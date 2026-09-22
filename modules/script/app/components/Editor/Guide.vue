<script setup lang="ts">
import { useWorkbenchStore } from '#script/stores/workbench'
const wb = useWorkbenchStore()
</script>
<template>
  <div class="qsc-guide">
    <div>
      <h3>Write an indicator</h3>
      <p>QuantScript runs Python indicators shared by QuantSystems and QuantAlgo.</p>
    </div>
    <section>
      <h4>01 · Compute a signal</h4>
      <p>
        Implement <code>_compute(df)</code> on a <code>TrendIndicator</code>. The frame contains open, high, low and
        close prices with a time index; market data can also include volume.
      </p>
      <pre class="qsc-pre">
def _compute(self, df):
    close = df["close"]
    # Return one value per bar
    return signals</pre
      >
    </section>
    <section>
      <h4>02 · Follow the contract</h4>
      <div class="qsc-signal-key">
        <span><b>+1</b> Uptrend</span><span><b>−1</b> Downtrend</span><span><b>0</b> Warm-up only</span>
      </div>
      <p>
        Once the signal leaves zero, stay in +1 or −1. Use only information available at each bar. Rescaling prices must
        leave the signal unchanged.
      </p>
    </section>
    <section>
      <h4>03 · Register the class</h4>
      <pre class="qsc-pre">
REGISTER = {"my_trend": MyTrend}
WARMUP = {"my_trend": 80}</pre
      >
      <p>The new-script template includes registration and a working EMA implementation.</p>
    </section>
    <section>
      <h4>04 · Check, save, validate</h4>
      <p>
        <b>Run check</b> tests the current editor buffer. <b>Save</b> checks the file and records a version. The
        <b>Forge</b> evaluates saved indicators against market data.
      </p>
      <p>
        A successful code check is not a certification. After changing a script, validate its evidence again. Restart a
        running QuantSystems engine to load the new code.
      </p>
    </section>
    <button class="qsc-btn is-sm" @click="wb.openScript('contract.py')">Read the full contract</button>
    <section>
      <h4>Shortcuts</h4>
      <dl>
        <dt>Save</dt>
        <dd>Ctrl+S</dd>
        <dt>Full check</dt>
        <dd>Ctrl+Enter</dd>
        <dt>Script sidebar</dt>
        <dd>Ctrl+B</dd>
        <dt>Inspector</dt>
        <dd>Ctrl+Shift+B</dd>
        <dt>Find in code</dt>
        <dd>Ctrl+F</dd>
      </dl>
    </section>
  </div>
</template>
<style scoped>
.qsc-guide {
  display: flex;
  flex-direction: column;
  gap: 22px;
  font-size: 12px;
}
.qsc-guide h3 {
  font-size: 15px;
  font-weight: 600;
}
.qsc-guide h4 {
  font-size: 12px;
  font-weight: 600;
  margin-bottom: 8px;
}
.qsc-guide p {
  color: var(--qss-text-secondary);
  line-height: 1.7;
  margin-top: 7px;
}
.qsc-guide code {
  font-family: var(--qss-font-mono);
  font-size: 11px;
}
.qsc-guide .qsc-pre {
  font-size: 10px;
  padding: 9px;
  margin: 8px 0;
}
.qsc-signal-key {
  display: flex;
  flex-direction: column;
  gap: 4px;
  color: var(--qss-text-secondary);
}
.qsc-signal-key b {
  display: inline-block;
  width: 28px;
  color: var(--qss-text);
  font-family: var(--qss-font-mono);
}
.qsc-guide dl {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 6px;
  color: var(--qss-text-secondary);
  font-size: 11px;
}
.qsc-guide dd {
  font-family: var(--qss-font-mono);
  color: var(--qss-text-muted);
}
</style>
