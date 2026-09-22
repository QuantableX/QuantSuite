"""The engine: a microphone recorder and a Whisper transcriber.

Recording is cheap and needs no model, so ``start`` always works; the model
loads (and, the first time, downloads) in the background and the final
transcription simply waits for it. While recording, a preview pass runs
over the last few seconds so the user sees words arriving; the real result
is one careful pass over the whole take when it stops.

Nothing here touches stdout — the RPC layer owns the wire — everything
outward goes through the ``notify`` callback as ``(method, params)``.
"""

from __future__ import annotations

import os
import sys
import threading
import time
from pathlib import Path
from typing import Any, Callable

import numpy as np

SAMPLE_RATE = 16_000
BLOCK = 1024
BANDS = 12
LEVEL_INTERVAL = 0.08
PREVIEW_INTERVAL = 2.0
# CPU: the preview sees the tail only and decodes greedily — a pass has to
# stay well under the interval.
PREVIEW_WINDOW_CPU_SEC = 14.0
# GPU: the whole take (up to this) with the final pass's settings, so the
# preview reads like the result. A tail window starts mid-sentence and
# re-detects the language on every pass — on a German/English take the
# words shown differed visibly from what was said and from the result.
PREVIEW_WINDOW_GPU_SEC = 60.0
PREVIEW_MIN_SEC = 0.8
MAX_RECORD_SEC = 900
MODEL_WAIT_SEC = 600
# `auto` on a GPU that needs a first-time kernel build: this long, then the CPU.
CUDA_WARMUP_BUDGET_SEC = 240.0

# faster-whisper's own repository map, kept here so a download can report
# progress (its helper hides the huggingface call).
MODELS: dict[str, str] = {
    "tiny": "Systran/faster-whisper-tiny",
    "base": "Systran/faster-whisper-base",
    "small": "Systran/faster-whisper-small",
    "medium": "Systran/faster-whisper-medium",
    "large-v2": "Systran/faster-whisper-large-v2",
    "large-v3": "Systran/faster-whisper-large-v3",
    "large-v3-turbo": "deepdml/faster-whisper-large-v3-turbo-ct2",
    "turbo": "deepdml/faster-whisper-large-v3-turbo-ct2",
    "distil-large-v3": "Systran/faster-distil-whisper-large-v3",
}
MODEL_FILES = ["config.json", "preprocessor_config.json", "model.bin", "tokenizer.json", "vocabulary.*"]

Notify = Callable[[str, dict[str, Any]], None]


def _site_packages() -> list[Path]:
    out: list[Path] = []
    for p in sys.path:
        if p and p.lower().endswith("site-packages") and Path(p).is_dir():
            out.append(Path(p))
    return out


def add_cuda_dll_dirs() -> list[str]:
    """Make the pip-installed CUDA runtime findable (Windows) before ctranslate2 loads."""
    added: list[str] = []
    for site in _site_packages():
        nvidia = site / "nvidia"
        if not nvidia.is_dir():
            continue
        for pkg in ("cublas", "cudnn", "cuda_runtime", "cuda_nvrtc"):
            bin_dir = nvidia / pkg / "bin"
            if bin_dir.is_dir():
                added.append(str(bin_dir))
                if hasattr(os, "add_dll_directory"):
                    try:
                        os.add_dll_directory(str(bin_dir))
                    except OSError:
                        pass
    if added:
        os.environ["PATH"] = os.pathsep.join(added + [os.environ.get("PATH", "")])
    return added


def _band_edges() -> np.ndarray:
    # Log-spaced from 80 Hz to 6 kHz over the rfft bins of one block.
    freqs = np.fft.rfftfreq(BLOCK, 1.0 / SAMPLE_RATE)
    edges = np.geomspace(80.0, 6000.0, BANDS + 1)
    return np.searchsorted(freqs, edges)


_EDGES = _band_edges()
_WINDOW = np.hanning(BLOCK).astype(np.float32)


def levels(block: np.ndarray) -> dict[str, Any]:
    """RMS, peak and a small log spectrum of one block, all 0..1."""
    if block.size == 0:
        return {"rms": 0.0, "peak": 0.0, "bands": [0.0] * BANDS}
    rms = float(np.sqrt(np.mean(block * block)))
    peak = float(np.max(np.abs(block)))
    if block.size == BLOCK:
        mag = np.abs(np.fft.rfft(block * _WINDOW)) / (BLOCK / 4)
    else:
        mag = np.abs(np.fft.rfft(block)) / (max(block.size, 1) / 4)
    bands: list[float] = []
    for i in range(BANDS):
        lo, hi = int(_EDGES[i]), int(max(_EDGES[i + 1], _EDGES[i] + 1))
        seg = mag[lo:hi] if lo < mag.size else np.zeros(1)
        db = 20.0 * np.log10(float(np.mean(seg)) + 1e-6)
        bands.append(float(np.clip((db + 54.0) / 54.0, 0.0, 1.0)))
    return {
        "rms": float(np.clip(rms * 6.0, 0.0, 1.0)),
        "peak": float(np.clip(peak * 1.5, 0.0, 1.0)),
        "bands": bands,
    }


class Recorder:
    """16 kHz mono float32 from one input device into memory."""

    def __init__(self, on_level: Callable[[dict[str, Any]], None], device: int | str | None = None) -> None:
        self._on_level = on_level
        self._device = device
        self._chunks: list[np.ndarray] = []
        self._lock = threading.Lock()
        self._stream: Any = None
        self._last_level = 0.0
        self.samples = 0
        self.overflow = False
        self.started_at = 0.0

    def start(self) -> None:
        import sounddevice as sd

        self._stream = sd.InputStream(
            samplerate=SAMPLE_RATE,
            channels=1,
            dtype="float32",
            blocksize=BLOCK,
            device=self._device,
            callback=self._callback,
        )
        self._stream.start()
        self.started_at = time.time()

    def _callback(self, indata: np.ndarray, frames: int, _time: Any, _status: Any) -> None:
        block = np.ascontiguousarray(indata[:, 0], dtype=np.float32).copy()
        with self._lock:
            if self.samples < MAX_RECORD_SEC * SAMPLE_RATE:
                self._chunks.append(block)
                self.samples += block.size
            else:
                self.overflow = True
        now = time.time()
        if now - self._last_level >= LEVEL_INTERVAL:
            self._last_level = now
            try:
                self._on_level(levels(block))
            except Exception:  # noqa: BLE001 - the meter must never kill the stream
                pass

    def seconds(self) -> float:
        return self.samples / SAMPLE_RATE

    def audio(self) -> np.ndarray:
        with self._lock:
            if not self._chunks:
                return np.zeros(0, dtype=np.float32)
            return np.concatenate(self._chunks)

    def tail(self, seconds: float) -> np.ndarray:
        want = int(seconds * SAMPLE_RATE)
        with self._lock:
            out: list[np.ndarray] = []
            have = 0
            for chunk in reversed(self._chunks):
                out.append(chunk)
                have += chunk.size
                if have >= want:
                    break
            if not out:
                return np.zeros(0, dtype=np.float32)
            audio = np.concatenate(list(reversed(out)))
        return audio[-want:] if audio.size > want else audio

    def stop(self) -> np.ndarray:
        stream, self._stream = self._stream, None
        if stream is not None:
            try:
                stream.stop()
                stream.close()
            except Exception:  # noqa: BLE001
                pass
        return self.audio()


class Engine:
    def __init__(self, notify: Notify) -> None:
        self._notify = notify
        self._lock = threading.Lock()
        self.model: Any = None
        self.model_name = "small"
        self.device_wanted = "auto"
        self.device: str | None = None
        self.compute: str | None = None
        self.language: str | None = None
        self.live = True
        self.input_device: str | None = None
        self.loading = False
        self.load_error: str | None = None
        self._pending_load: tuple[str, str] | None = None
        self._rec: Recorder | None = None
        self._gen = 0
        self._preview_busy = False
        self._finals = 0  # final passes in flight: the take is still "transcribing"
        self.model_dir = Path(os.environ.get("QUANTVOICE_MODEL_DIR") or (Path.home() / ".quantsuite" / "modules" / "hud" / "models"))
        self.cuda_dirs = add_cuda_dll_dirs()

    # ── state ──

    def info(self) -> dict[str, Any]:
        return {
            "loaded": self.model is not None,
            "loading": self.loading,
            "loadError": self.load_error,
            "model": self.model_name,
            "device": self.device,
            "deviceWanted": self.device_wanted,
            "compute": self.compute,
            "language": self.language,
            "live": self.live,
            "inputDevice": self.input_device,
            "recording": self._rec is not None,
            "modelDir": str(self.model_dir),
            "cudaDlls": bool(self.cuda_dirs),
        }

    def _state(self, phase: str, **extra: Any) -> None:
        # `recording`/`transcribing` ride along on every state, so the host
        # never infers a take from the phase: a model that finished loading
        # mid-take used to announce `ready`, the host forgot the take, and
        # the stop then failed with "not recording".
        params: dict[str, Any] = {
            "phase": phase,
            "model": self.model_name,
            "device": self.device,
            "compute": self.compute,
            "recording": self._rec is not None,
            "transcribing": self._finals > 0,
        }
        params.update(extra)
        self._notify("state", params)

    def take_phase(self) -> str | None:
        """The phase of a take in flight — it outranks whatever the model is doing."""
        if self._rec is not None:
            return "recording"
        if self._finals > 0:
            return "transcribing"
        return None

    def idle_phase(self) -> str:
        take = self.take_phase()
        if take:
            return take
        if self.loading:
            return "loading"
        if self.model is None:
            return "error" if self.load_error else "unloaded"
        return "ready"

    # ── configuration and model loading ──

    def configure(self, params: dict[str, Any]) -> dict[str, Any]:
        model = str(params.get("model") or self.model_name).strip().lower()
        device = str(params.get("device") or self.device_wanted).strip().lower()
        if "language" in params:
            lang = params.get("language")
            self.language = None if not lang or str(lang).lower() in ("auto", "system", "") else str(lang).lower()
        if "live" in params:
            self.live = bool(params.get("live"))
        if "inputDevice" in params:
            dev = params.get("inputDevice")
            self.input_device = str(dev) if dev else None
        needs_load = self.model is None or model != self.model_name or device != self.device_wanted
        self.model_name = model
        self.device_wanted = device
        if needs_load:
            self.load_error = None
            self._start_load(model, device)
        return self.info()

    def _start_load(self, model: str, device: str) -> None:
        with self._lock:
            if self.loading:
                self._pending_load = (model, device)
                return
            self.loading = True
        threading.Thread(target=self._load, args=(model, device), name="quantvoice-load", daemon=True).start()

    def _load(self, model: str, device: str) -> None:
        try:
            self._state("loading", detail=f"loading {model}")
            path = self._download(model)
            self._state("loading", detail=f"loading {model} on {device}")
            loaded, dev, compute = self._open(path, device)
            if dev == "cuda":
                # The first CUDA pass may JIT-compile every kernel (a wheel
                # without SASS for this GPU): minutes, once, then cached by
                # the driver. Better spent here than on the user's first take.
                # `auto` gives it a bounded wait and falls back to the CPU;
                # an explicit `cuda` waits as long as it takes.
                self._state("loading", detail=f"warming up {model} on the GPU (the first time can take minutes)")
                budget = None if device == "cuda" else CUDA_WARMUP_BUDGET_SEC
                if not self._warm_up(loaded, budget):
                    self._state("loading", detail=f"the GPU did not warm up within {CUDA_WARMUP_BUDGET_SEC:.0f}s — using the CPU")
                    loaded, dev, compute = self._open(path, "cpu")
            self.model, self.device, self.compute = loaded, dev, compute
            self.load_error = None
            self._state(self.take_phase() or "ready")
        except Exception as exc:  # noqa: BLE001
            self.model = None
            self.load_error = str(exc)
            self._state("error", detail=str(exc))
        finally:
            with self._lock:
                self.loading = False
                pending, self._pending_load = self._pending_load, None
            if pending and pending != (model, device):
                self._start_load(*pending)
            else:
                take = self.take_phase()
                if take:
                    # A take that started while loading: tell the UI it is still on.
                    self._state(take)

    def _download(self, model: str) -> str:
        """The model directory, downloading it into the module's model dir first."""
        if os.path.isdir(model):
            return model
        repo = MODELS.get(model, model)
        self.model_dir.mkdir(parents=True, exist_ok=True)
        from huggingface_hub import snapshot_download
        from huggingface_hub.utils import tqdm as hf_tqdm

        notify = self._notify
        state = {"last": 0.0}

        class Progress(hf_tqdm):  # type: ignore[misc]
            def update(self, n: float | None = 1) -> Any:
                result = super().update(n)
                now = time.time()
                if now - state["last"] >= 0.25 or (self.total and self.n >= self.total):
                    state["last"] = now
                    notify(
                        "download",
                        {
                            "model": model,
                            "file": str(getattr(self, "desc", "") or ""),
                            "done": int(self.n or 0),
                            "total": int(self.total or 0),
                        },
                    )
                return result

        # Everything cached already → returns at once without network.
        try:
            return snapshot_download(repo, allow_patterns=MODEL_FILES, cache_dir=str(self.model_dir), local_files_only=True)
        except Exception:  # noqa: BLE001 - not cached, go online
            pass
        self._state("downloading", detail=f"downloading {model}")
        return snapshot_download(repo, allow_patterns=MODEL_FILES, cache_dir=str(self.model_dir), tqdm_class=Progress)

    @staticmethod
    def _warm_up(model: Any, budget: float | None) -> bool:
        """One second of silence through the model; ``False`` when it did not return in time."""

        def run() -> None:
            silence = np.zeros(SAMPLE_RATE, dtype=np.float32)
            segments, _ = model.transcribe(silence, beam_size=1, vad_filter=False, without_timestamps=True, language="en")
            for _ in segments:
                pass

        if budget is None:
            run()
            return True
        # A CUDA call cannot be interrupted from Python; the thread is left
        # to finish on its own if the budget runs out.
        worker = threading.Thread(target=run, name="quantvoice-warmup", daemon=True)
        worker.start()
        worker.join(budget)
        return not worker.is_alive()

    def _open(self, path: str, device: str) -> tuple[Any, str, str]:
        from faster_whisper import WhisperModel

        # `auto`: the GPU first, for every model, but only with the pip CUDA
        # runtime in the venv — a cuDNN found on the system PATH of the wrong
        # major version hangs the first pass instead of failing. Without the
        # wheels the CPU (int8) it is. (Keeping tiny/base/small on the CPU
        # cost 2–2.5 s per pass with auto-detect against 0.1–0.4 s on the GPU.)
        cuda_first = device == "cuda" or (device == "auto" and bool(self.cuda_dirs))
        attempts: list[tuple[str, str]] = []
        if device in ("auto", "cuda") and cuda_first:
            attempts.append(("cuda", "float16"))
        if device in ("auto", "cpu"):
            attempts.append(("cpu", "int8"))
        if device == "auto" and not cuda_first:
            attempts.append(("cuda", "float16"))
        errors: list[str] = []
        for dev, compute in attempts:
            if dev == "cuda":
                try:
                    import ctranslate2

                    if ctranslate2.get_cuda_device_count() < 1:
                        errors.append("cuda: no CUDA device visible to CTranslate2 (missing cuBLAS/cuDNN DLLs or driver)")
                        continue
                except Exception as exc:  # noqa: BLE001
                    errors.append(f"cuda: {exc}")
                    continue
            try:
                model = WhisperModel(path, device=dev, compute_type=compute)
                return model, dev, compute
            except Exception as exc:  # noqa: BLE001
                errors.append(f"{dev}/{compute}: {exc}")
        raise RuntimeError("; ".join(errors) or "no device to load the model on")

    def unload(self) -> dict[str, Any]:
        self.model = None
        self.device = None
        self.compute = None
        self._state("unloaded")
        return self.info()

    # ── recording ──

    def _resolve_input(self) -> int | None:
        if not self.input_device:
            return None
        import sounddevice as sd

        wanted = self.input_device.strip().lower()
        # The same microphone appears once per host API; WASAPI carries the
        # full name (MME truncates to 31 characters) and is the better path.
        matches: list[tuple[int, str]] = []
        for idx, dev in enumerate(sd.query_devices()):
            if int(dev.get("max_input_channels", 0)) <= 0:
                continue
            name = str(dev.get("name", "")).strip().lower()
            if name == wanted or (len(name) >= 31 and wanted.startswith(name)):
                api = str(sd.query_hostapis(int(dev.get("hostapi", 0))).get("name", "")).lower()
                matches.append((idx, api))
        if not matches:
            return None
        for idx, api in matches:
            if "wasapi" in api:
                return idx
        return matches[0][0]

    def start(self, params: dict[str, Any]) -> dict[str, Any]:
        with self._lock:
            if self._rec is not None:
                raise RuntimeError("already recording")
            self._gen += 1
            gen = self._gen
            rec = Recorder(lambda lv: self._notify("level", lv), device=self._resolve_input())
            rec.start()
            self._rec = rec
        self._state("recording")
        if self.live:
            threading.Thread(target=self._preview_loop, args=(gen,), name="quantvoice-preview", daemon=True).start()
        return {"ok": True, "gen": gen}

    def stop(self, params: dict[str, Any]) -> dict[str, Any]:
        with self._lock:
            rec, self._rec = self._rec, None
            gen = self._gen
        if rec is None:
            raise RuntimeError("not recording")
        audio = rec.stop()
        seconds = audio.size / SAMPLE_RATE
        if seconds < 0.25:
            self._notify("result", {"text": "", "language": None, "probability": 0.0, "durationSec": seconds, "elapsedSec": 0.0, "gen": gen, "empty": True})
            self._state(self.idle_phase())
            return {"ok": True, "seconds": seconds}
        with self._lock:
            self._finals += 1
        self._state("transcribing", detail=f"{seconds:.1f}s of audio")
        threading.Thread(target=self._final, args=(audio, gen), name="quantvoice-final", daemon=True).start()
        return {"ok": True, "seconds": seconds}

    def cancel(self, _params: dict[str, Any]) -> dict[str, Any]:
        with self._lock:
            rec, self._rec = self._rec, None
            self._gen += 1
        if rec is not None:
            rec.stop()
        self._state(self.idle_phase())
        return {"ok": True}

    # ── transcription ──

    def _wait_model(self) -> None:
        deadline = time.time() + MODEL_WAIT_SEC
        while self.model is None:
            if self.load_error and not self.loading:
                raise RuntimeError(self.load_error)
            if not self.loading and self.model is None:
                self._start_load(self.model_name, self.device_wanted)
            if time.time() > deadline:
                raise RuntimeError("the model did not load in time")
            time.sleep(0.2)

    def _transcribe(self, audio: np.ndarray, final: bool) -> tuple[str, Any]:
        self._wait_model()
        # On the GPU a preview affords the final pass's settings (a few
        # hundred ms a take); on the CPU it is greedy and skips the VAD.
        careful = final or self.device == "cuda"
        segments, info = self.model.transcribe(
            audio,
            language=self.language,
            beam_size=5 if careful else 1,
            best_of=5 if careful else 1,
            vad_filter=careful,
            vad_parameters={"min_silence_duration_ms": 400, "speech_pad_ms": 200},
            condition_on_previous_text=False,
            without_timestamps=True,
        )
        parts = [seg.text.strip() for seg in segments]
        text = " ".join(p for p in parts if p).strip()
        return text, info

    def _final(self, audio: np.ndarray, gen: int) -> None:
        started = time.time()
        try:
            text, info = self._transcribe(audio, final=True)
            self._notify(
                "result",
                {
                    "text": text,
                    "language": getattr(info, "language", None),
                    "probability": float(getattr(info, "language_probability", 0.0) or 0.0),
                    "durationSec": float(audio.size / SAMPLE_RATE),
                    "elapsedSec": float(time.time() - started),
                    "gen": gen,
                    "empty": not text,
                },
            )
        except Exception as exc:  # noqa: BLE001
            self._notify("error", {"message": f"transcription failed: {exc}", "gen": gen})
        finally:
            with self._lock:
                self._finals -= 1
            self._state(self.idle_phase())

    def _preview_loop(self, gen: int) -> None:
        while True:
            time.sleep(PREVIEW_INTERVAL)
            rec = self._rec
            if rec is None or gen != self._gen:
                return
            if self.model is None or self._preview_busy:
                continue
            audio = rec.tail(PREVIEW_WINDOW_GPU_SEC if self.device == "cuda" else PREVIEW_WINDOW_CPU_SEC)
            if audio.size < PREVIEW_MIN_SEC * SAMPLE_RATE:
                continue
            self._preview_busy = True
            try:
                text, _ = self._transcribe(audio, final=False)
                if gen == self._gen and self._rec is not None:
                    self._notify("interim", {"text": text, "gen": gen, "seconds": rec.seconds()})
            except Exception:  # noqa: BLE001 - a preview that fails is just skipped
                pass
            finally:
                self._preview_busy = False

    def transcribe_file(self, params: dict[str, Any]) -> dict[str, Any]:
        """A file instead of the microphone — the self-test, and a debugging aid."""
        path = str(params.get("path") or "")
        if not path or not os.path.isfile(path):
            raise RuntimeError(f"no such file: {path}")
        from faster_whisper.audio import decode_audio

        audio = decode_audio(path, sampling_rate=SAMPLE_RATE)
        started = time.time()
        text, info = self._transcribe(np.asarray(audio, dtype=np.float32), final=True)
        return {
            "text": text,
            "language": getattr(info, "language", None),
            "probability": float(getattr(info, "language_probability", 0.0) or 0.0),
            "durationSec": float(audio.size / SAMPLE_RATE),
            "elapsedSec": float(time.time() - started),
            "device": self.device,
            "model": self.model_name,
        }

    # ── devices ──

    @staticmethod
    def devices() -> list[dict[str, Any]]:
        import sounddevice as sd

        default_in = sd.default.device[0] if isinstance(sd.default.device, (list, tuple)) else sd.default.device
        out: list[dict[str, Any]] = []
        for idx, dev in enumerate(sd.query_devices()):
            if int(dev.get("max_input_channels", 0)) <= 0:
                continue
            out.append(
                {
                    "index": idx,
                    "name": str(dev.get("name", "")),
                    "channels": int(dev.get("max_input_channels", 0)),
                    "default": idx == default_in,
                    "hostapi": str(sd.query_hostapis(int(dev.get("hostapi", 0))).get("name", "")),
                }
            )
        return out
