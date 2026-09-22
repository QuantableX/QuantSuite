<template>
  <div
    class="app-wrapper"
    :class="{
      tucked: isTucked,
      'position-right': windowPosition === 'right',
      'position-top': isTop,
    }"
    :data-hud-section="activeModule"
    @mouseleave="onMouseLeave"
  >
    <!-- Trigger zone (left for right-position) -->
    <div
      v-if="windowPosition === 'right'"
      class="trigger-zone trigger-left"
    >
      <HudTriggerTab
        side="left"
        :tucked="isTucked"
        :pinned="isPinned"
        @hover="activationMode === 'hover' ? onMouseEnter() : undefined"
        @activate="activationMode === 'click' ? onTriggerClick() : undefined"
      />
    </div>

    <!-- Main content area -->
    <div class="main-container">
      <!-- Header with Burger and Pin -->
      <header v-if="isTop" class="top-header">
        <button class="top-brand" aria-label="QuantHUD home" @click="activeModule = 'home'">
          <PanelsTopLeft :size="21" :stroke-width="1.6" /><span>Quant<span class="brand-suffix">HUD</span></span>
        </button>
        <nav class="top-module-nav" aria-label="HUD modules" @wheel="scrollModuleNav">
          <button class="top-module-tab" :class="{ active: activeModule === 'home' }" :aria-current="activeModule === 'home' ? 'page' : undefined" data-module-id="home" @click="activeModule = 'home'">
            <LayoutGrid :size="15" /><span>Overview</span>
          </button>
          <button v-for="mod in topModules" :key="mod.id" class="top-module-tab" :class="{ active: activeModule === mod.id }"
            :aria-current="activeModule === mod.id ? 'page' : undefined" :data-module-id="mod.id" @click="activeModule = mod.id">
            <span class="top-nav-icon" v-html="mod.icon" /><span>{{ mod.label }}</span>
          </button>
          <button v-if="activeModule === 'chart-analyzer-history'" class="top-module-tab active" data-module-id="chart-analyzer-history">Analysis history</button>
        </nav>
        <div class="top-header-actions">
          <button class="top-pin" :class="{ active: isPinned }" :aria-pressed="isPinned" aria-label="Pin window"
            title="Keep HUD open" @click="togglePin"><Pin :size="15" /><span>{{ isPinned ? 'Pinned' : 'Keep open' }}</span></button>
          <button class="btn btn-icon" :class="{ active: activeModule === 'settings' }" aria-label="Settings" data-module-id="settings"
            title="Settings" @click="activeModule = 'settings'"><Settings2 :size="18" /></button>
        </div>
      </header>
      <header v-else class="header">
          <!-- Left side: Burger when left, Pin when right -->
          <HudSidebar
            v-if="windowPosition === 'left'"
            :active-module="activeModule"
            :window-position="windowPosition"
            :display-mode="config.displayMode || 'basic'"
            @update:active-module="activeModule = $event"
          />
          <button
            v-else
            class="btn btn-icon pin-btn"
            :class="{ active: isPinned }"
            @click="togglePin"
            title="Pin window (keeps it visible)"
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <line x1="12" y1="17" x2="12" y2="22" />
              <path
                d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z"
              />
            </svg>
          </button>

          <img
            src="/hud/QuantHUD.png"
            alt="QuantHUD"
            class="title-logo"
            @click="activeModule = 'home'"
            style="cursor: pointer"
          />

          <!-- Right side: Pin when left, Burger when right -->
          <button
            v-if="windowPosition === 'left'"
            class="btn btn-icon pin-btn"
            :class="{ active: isPinned }"
            @click="togglePin"
            title="Pin window (keeps it visible)"
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <line x1="12" y1="17" x2="12" y2="22" />
              <path
                d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z"
              />
            </svg>
          </button>
          <HudSidebar
            v-else
            :active-module="activeModule"
            :window-position="windowPosition"
            :display-mode="config.displayMode || 'basic'"
            @update:active-module="activeModule = $event"
          />
      </header>

      <!-- Scrollable Content -->
      <HudContentViewport :horizontal="isTop" :section="activeModule">
        <!-- Home Module -->
        <div v-if="activeModule === 'home'" class="module-content">
          <div v-if="isTop" class="top-launcher">
            <div class="launcher-intro">
              <span class="hud-eyebrow">YOUR WORKSPACE</span>
              <h1>Quick access</h1>
              <p>Everyday essentials.<br />Right where you need them.</p>
              <span class="launcher-count">{{ topModules.length }} tools available <ArrowUpRight :size="13" /></span>
            </div>
            <section v-for="group in launcherGroups" :key="group.label" class="launcher-group">
              <h2>{{ group.label }}<span>{{ group.modules.length.toString().padStart(2, '0') }}</span></h2>
              <div class="launcher-items">
                <button v-for="mod in group.modules" :key="mod.id" class="launcher-item" @click="activeModule = mod.id">
                  <span class="launcher-icon" v-html="mod.icon" />
                  <span class="launcher-copy"><strong>{{ mod.label }}</strong><span>{{ moduleDescriptions[mod.id] }}</span></span>
                  <ArrowUpRight class="launcher-arrow" :size="15" />
                </button>
              </div>
            </section>
          </div>
          <div v-else class="home-hub">
            <!-- Basic mode: only general modules -->
            <template v-if="displayMode === 'basic'">
              <div class="hub-grid">
                <button
                  v-for="mod in homeGeneralModules"
                  :key="mod.id"
                  class="hub-card"
                  @click="activeModule = mod.id"
                >
                  <span class="hub-icon" v-html="mod.icon"></span>
                  <span class="hub-label">{{ mod.label }}</span>
                </button>
              </div>
            </template>

            <!-- Pro mode: sections with collapsible dividers -->
            <template v-else>
              <div
                class="home-section-divider"
                @click="homeGeneralCollapsed = !homeGeneralCollapsed"
              >
                <span class="home-divider-line"></span>
                <span class="home-divider-label"
                  >{{ homeGeneralCollapsed ? "▶" : "▼" }} General</span
                >
                <span class="home-divider-line"></span>
              </div>
              <div v-if="!homeGeneralCollapsed" class="hub-grid">
                <button
                  v-for="mod in homeGeneralModules"
                  :key="mod.id"
                  class="hub-card"
                  @click="activeModule = mod.id"
                >
                  <span class="hub-icon" v-html="mod.icon"></span>
                  <span class="hub-label">{{ mod.label }}</span>
                </button>
              </div>

              <div
                class="home-section-divider"
                @click="homeAdvancedCollapsed = !homeAdvancedCollapsed"
              >
                <span class="home-divider-line"></span>
                <span class="home-divider-label"
                  >{{ homeAdvancedCollapsed ? "▶" : "▼" }} Advanced</span
                >
                <span class="home-divider-line"></span>
              </div>
              <div v-if="!homeAdvancedCollapsed" class="hub-grid">
                <button
                  v-for="mod in homeAdvancedModules"
                  :key="mod.id"
                  class="hub-card"
                  @click="activeModule = mod.id"
                >
                  <span class="hub-icon" v-html="mod.icon"></span>
                  <span class="hub-label">{{ mod.label }}</span>
                </button>
              </div>
            </template>
          </div>
        </div>

        <!-- Notes Module -->
        <div v-else-if="activeModule === 'notes'" class="module-content">
          <HudNotesModule :horizontal="isTop" />
        </div>

        <!-- Position Size Calculator Module -->
        <div v-else-if="activeModule === 'position-calc'" class="position-calc-content">
          <div class="position-levels">
          <!-- Capture Controls -->
          <div class="capture-row">
            <button
              class="btn btn-primary capture-btn"
              @click="handleCapture"
              :disabled="isProcessing"
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                style="margin-right: 6px"
              >
                <path
                  d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z"
                />
                <circle cx="12" cy="13" r="4" />
              </svg>
              Capture (F9)
            </button>
            <button
              class="btn btn-icon region-btn"
              :class="{ active: scanRegion }"
              @click="toggleRegion"
              title="Select scan region"
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <rect x="3" y="3" width="7" height="7" />
                <rect x="14" y="3" width="7" height="7" />
                <rect x="3" y="14" width="7" height="7" />
                <rect x="14" y="14" width="7" height="7" />
              </svg>
            </button>
          </div>

          <!-- Status -->
          <div class="status">{{ status }}</div>

          <!-- Long/Short Toggle -->
          <div class="direction-toggle">
            <button
              class="btn direction-btn long"
              :class="{ active: isLong }"
              @click="setDirection(true)"
            >
              ▲ LONG ▲
            </button>
            <button class="btn btn-icon btn-red" @click="handleClear">
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
            <button
              class="btn direction-btn short"
              :class="{ active: !isLong }"
              @click="setDirection(false)"
            >
              ▼ SHORT ▼
            </button>
          </div>

          <!-- Levels Card -->
          <HudLevelsCard
            :is-long="isLong"
            :levels="levels"
            @update:levels="updateLevels"
            @copy="copyToClipboard"
          />

          </div>

          <!-- Calculator Card -->
          <HudCalculatorCard
            :inputs="inputs"
            @update:inputs="updateInputs"
            @calculate="calculate"
          />

          <!-- Results Card -->
          <HudResultsCard
            :results="results"
            :leverage="inputs.leverage"
            :error="error"
            @copy="copyToClipboard"
          />
        </div>

        <!-- Chart Analyzer Module -->
        <div
          v-else-if="activeModule === 'chart-analyzer'"
          class="module-content module-content--fill"
        >
          <HudChartAnalyzerModule @open-history="activeModule = 'chart-analyzer-history'" />
        </div>

        <!-- Chart Analyzer History -->
        <div
          v-else-if="activeModule === 'chart-analyzer-history'"
          class="module-content module-content--fill"
        >
          <HudChartAnalyzerHistory @back="activeModule = 'chart-analyzer'" />
        </div>

        <!-- Settings Module -->
        <div v-else-if="activeModule === 'settings'" class="module-content settings-content">
          <div class="card settings-shell">
          <nav v-if="isTop" class="settings-nav" aria-label="Settings categories">
            <button v-for="tab in settingsTabs" :key="tab.id" :class="{ active: settingsSection === tab.id }" :aria-pressed="settingsSection === tab.id" @click="settingsSection = tab.id">
              <component :is="tab.icon" :size="15" /><span>{{ tab.label }}</span><ChevronRight :size="13" />
            </button>
          </nav>
          <div v-if="!isTop || settingsSection === 'about'" class="top-settings-group" data-settings-section="about">
            <div v-if="isTop" class="settings-intro"><span class="hud-eyebrow">PREFERENCES</span><h2>About QuantHUD</h2><p>Your everyday tools, one gesture away.</p><span class="settings-autosave">Changes save automatically</span></div>
            <div class="version-badge">QuantSuite v{{ appVersion }}</div>

          </div>
          <div v-if="!isTop || settingsSection === 'display'" class="top-settings-group" data-settings-section="display">
            <div v-if="isTop" class="settings-intro"><span class="hud-eyebrow">PREFERENCES</span><h2>Make it yours</h2><p>Choose where your HUD lives and how it looks.</p><span class="settings-autosave">Changes save automatically</span></div>
            <!-- Monitor Selection -->
            <div class="setting-group">
              <label class="setting-label" for="hud-setting-1">Monitor</label>
              <select id="hud-setting-1"
                class="monitor-select"
                :value="config.monitorIndex"
                @change="handleMonitorChange($event)"
              >
                <option
                  v-for="monitor in availableMonitors"
                  :key="monitor.index"
                  :value="monitor.index"
                >
                  {{ monitor.name }}
                </option>
              </select>
              <div class="setting-hint">Choose the display where your HUD appears.</div>
            </div>

            <!-- Window Position -->
            <div class="setting-group">
              <label class="setting-label" for="hud-setting-2">Window Position</label>
              <select id="hud-setting-2"
                class="monitor-select"
                :value="config.windowPosition || 'left'"
                @change="
                  handlePositionChange(
                    ($event.target as HTMLSelectElement).value as any,
                  )
                "
              >
                <option value="left">◀ Left</option>
                <option value="right">Right ▶</option>
                <option value="dual">◀ Dual ▶</option>
                <option value="top">▲ Top</option>
              </select>
              <div class="setting-hint">A bar along the top, or a panel at either side.</div>
            </div>

            <!-- Color Theme -->
            <div class="setting-group">
              <label class="setting-label" for="hud-setting-3">Color Theme</label>
              <select id="hud-setting-3"
                class="monitor-select"
                :value="config.colorTheme || 'dark'"
                @change="
                  handleThemeChange(
                    ($event.target as HTMLSelectElement).value as any,
                  )
                "
              >
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </select>
              <div class="setting-hint">Set the appearance of your HUD.</div>
            </div>

          </div>
          <div v-if="!isTop || settingsSection === 'behavior'" class="top-settings-group" data-settings-section="behavior">
            <div v-if="isTop" class="settings-intro"><span class="hud-eyebrow">PREFERENCES</span><h2>A natural workflow</h2><p>Choose how you open the HUD and which tools you see.</p><span class="settings-autosave">Changes save automatically</span></div>
            <!-- Activation Mode -->
            <div class="setting-group">
              <label class="setting-label" for="hud-setting-4">Activation Mode</label>
              <select id="hud-setting-4"
                class="monitor-select"
                :value="config.activationMode || 'hover'"
                @change="
                  handleActivationModeChange(
                    ($event.target as HTMLSelectElement).value as any,
                  )
                "
              >
                <option value="hover">Hover</option>
                <option value="click">Click</option>
              </select>
              <div class="setting-hint">Open the HUD with a click or by hovering over its tab.</div>
            </div>

            <!-- Display Mode -->
            <div class="setting-group">
              <label class="setting-label" for="hud-setting-5">Display Mode</label>
              <select id="hud-setting-5"
                class="monitor-select"
                :value="config.displayMode || 'basic'"
                @change="
                  handleDisplayModeChange(
                    ($event.target as HTMLSelectElement).value as any,
                  )
                "
              >
                <option value="basic">Basic</option>
                <option value="pro">Pro</option>
              </select>
              <div class="setting-hint">Pro adds the position sizer and chart analyzer.</div>
            </div>

            <!-- Screenshots Folder -->
            <div class="setting-group">
              <label class="setting-label">Screenshots Folder</label>
              <div class="folder-picker">
                <div
                  class="folder-path"
                  :title="
                    config.screenshotsFolder ||
                    defaultScreenshotsFolder ||
                    'Pictures/Screenshots'
                  "
                >
                  {{
                    config.screenshotsFolder ||
                    defaultScreenshotsFolder ||
                    "Pictures/Screenshots"
                  }}
                </div>
                <button
                  class="btn btn-ghost folder-browse-btn"
                  @click="browseScreenshotsFolder"
                >
                  Browse
                </button>
              </div>
              <button
                v-if="config.screenshotsFolder"
                class="btn btn-ghost"
                style="margin-top: 4px; font-size: 11px; padding: 3px 8px"
                @click="setScreenshotsFolder('')"
              >
                Reset to Default
              </button>
            </div>

          </div>
          <div v-if="!isTop || settingsSection === 'voice'" class="top-settings-group" data-settings-section="voice">
            <div v-if="isTop" class="settings-intro"><span class="hud-eyebrow">PREFERENCES</span><h2>Voice & language</h2><p>Set the language and quality of your transcriptions.</p><span class="settings-autosave">Changes save automatically</span></div>
            <!-- Whisper (QuantVoice, docs/PLAN-QUANTVOICE.md) -->
            <div class="setting-group">
              <label class="setting-label" for="hud-setting-6">Language</label>
              <select id="hud-setting-6"
                class="monitor-select"
                :value="speechLanguageCode(config.speechLanguage)"
                @change="onSpeechLanguage(($event.target as HTMLSelectElement).value)"
              >
                <option v-for="l in SPEECH_LANGUAGES" :key="l.code" :value="l.code">{{ l.label }}</option>
              </select>
              <div class="setting-hint">Auto-detect recognizes the language of each recording.</div>
            </div>

            <div class="setting-group">
              <label class="setting-label" for="hud-setting-7">Whisper Model</label>
              <select id="hud-setting-7"
                class="monitor-select"
                :value="config.speechModel || 'small'"
                @change="onSpeechModel(($event.target as HTMLSelectElement).value)"
              >
                <option v-for="m in SPEECH_MODELS" :key="m.id" :value="m.id">{{ m.label }}</option>
              </select>
              <div class="setting-hint">Larger models improve accuracy and take longer to process.</div>
            </div>

            <div class="setting-group">
              <label class="setting-label" for="hud-setting-8">Compute</label>
              <select id="hud-setting-8"
                class="monitor-select"
                :value="config.speechDevice || 'auto'"
                @change="onSpeechDevice(($event.target as HTMLSelectElement).value as any)"
              >
                <option value="auto">Auto · GPU preferred</option>
                <option value="cpu">CPU</option>
                <option value="cuda">GPU (CUDA)</option>
              </select>
              <div v-if="speechStatus?.device" class="setting-hint">
                Running on {{ speechStatus.device === 'cuda' ? 'the GPU' : 'the CPU' }} ({{ speechStatus.compute }}).
              </div>
            </div>

          </div>
          <div v-if="!isTop || settingsSection === 'controls'" class="top-settings-group" data-settings-section="controls">
            <div v-if="isTop" class="settings-intro"><span class="hud-eyebrow">PREFERENCES</span><h2>Recording controls</h2><p>Choose your microphone, shortcut and where words appear.</p><span class="settings-autosave">Changes save automatically</span></div>
            <div class="setting-group">
              <label class="setting-label" for="hud-setting-9">Hotkey Result</label>
              <select id="hud-setting-9"
                class="monitor-select"
                :value="config.speechInsert || 'paste'"
                @change="onSpeechInsert(($event.target as HTMLSelectElement).value as any)"
              >
                <option value="paste">Paste at cursor</option>
                <option value="type">Type at cursor</option>
                <option value="clipboard">Clipboard only</option>
              </select>
            </div>

            <div class="setting-group">
              <label class="setting-label" for="hud-setting-10">Push-to-talk Hotkey</label>
              <input id="hud-setting-10"
                class="monitor-select"
                :value="config.speechHotkey || DEFAULT_SPEECH_HOTKEY"
                placeholder="Ctrl+Shift+Space"
                spellcheck="false"
                @change="onSpeechHotkey(($event.target as HTMLInputElement).value)"
              />
              <div class="setting-hint">
                Hold to record. Tap to toggle.
              </div>
              <div v-if="speechStatus?.hotkeyError" class="setting-hint warn">Not registered: {{ speechStatus.hotkeyError }}</div>
            </div>

            <div class="setting-group">
              <label class="setting-label" for="hud-setting-11">Microphone</label>
              <select id="hud-setting-11"
                class="monitor-select"
                :value="config.speechInputDevice || ''"
                @focus="loadSpeechDevices()"
                @change="onSpeechInputDevice(($event.target as HTMLSelectElement).value)"
              >
                <option value="">System default</option>
                <option v-if="config.speechInputDevice && !micOptions.includes(config.speechInputDevice)" :value="config.speechInputDevice">{{ config.speechInputDevice }}</option>
                <option v-for="d in micOptions" :key="d" :value="d">{{ d }}</option>
              </select>
              <div v-if="!micOptions.length" class="setting-hint">Start the voice engine to discover microphones.</div>
            </div>

            <div class="setting-group">
              <span class="setting-label">Live preview</span>
              <label class="setting-checkbox">
                <input
                  type="checkbox"
                  :checked="config.speechLivePreview !== false"
                  @change="onSpeechLivePreview(($event.target as HTMLInputElement).checked)"
                />
                <span>Show words while recording</span>
              </label>
            </div>

            <!-- AI Vision Model (Chart Analyzer) -->
          </div>
          <div v-if="!isTop || settingsSection === 'analysis'" class="top-settings-group" data-settings-section="analysis">
            <div v-if="isTop" class="settings-intro"><span class="hud-eyebrow">PREFERENCES</span><h2>Chart analysis</h2><p>Connect a local vision model to analyze your captures.</p><span class="settings-autosave">Changes save automatically</span></div>
            <div class="setting-group">
              <label class="setting-label" for="hud-setting-12">AI Provider</label>
              <select id="hud-setting-12"
                class="monitor-select"
                :value="config.aiProvider || 'ollama'"
                @change="handleAiProviderChange(($event.target as HTMLSelectElement).value as any)"
              >
                <option value="ollama">Ollama</option>
                <option value="lmstudio">LM Studio</option>
              </select>
              <div class="setting-hint">Choose the app serving your local model.</div>
            </div>

            <div class="setting-group">
              <label class="setting-label" for="hud-setting-13">AI Base URL</label>
              <input id="hud-setting-13"
                class="input ai-url-input"
                :value="config.aiBaseUrl || ''"
                :placeholder="config.aiProvider === 'lmstudio' ? 'http://localhost:1234' : 'http://localhost:11434'"
                @change="setAiBaseUrl(($event.target as HTMLInputElement).value.trim())"
              />
              <div class="setting-hint">The address of your local model server.</div>
            </div>

            <div class="setting-group">
              <label class="setting-label" for="hud-setting-14">AI Vision Model</label>
              <input id="hud-setting-14"
                class="input ai-url-input"
                :value="config.aiModel || ''"
                :placeholder="config.aiProvider === 'lmstudio' ? 'model-name' : 'llava'"
                @change="setAiModel(($event.target as HTMLInputElement).value.trim())"
              />
              <div class="setting-hint">Use a model that supports image input.</div>
            </div>
          </div>
        </div>
        </div>

        <!-- Todo Module -->
        <div v-else-if="activeModule === 'todos'" class="module-content">
          <HudTodoModule />
        </div>

        <!-- World Clock Module -->
        <div
          v-else-if="activeModule === 'worldclock'"
          class="module-content module-content--fill"
        >
          <HudWorldClockModule />
        </div>

        <!-- Calendar Module -->
        <div v-else-if="activeModule === 'calendar'" class="module-content">
          <HudCalendarModule />
        </div>

        <!-- General Calculator Module -->
        <div
          v-else-if="activeModule === 'gen-calc'"
          class="module-content module-content--fill"
        >
          <HudGeneralCalcModule />
        </div>

        <!-- Color Picker Module -->
        <div v-else-if="activeModule === 'colorpicker'" class="module-content">
          <HudColorPickerModule />
        </div>

        <!-- Clipboard History Module -->
        <div
          v-else-if="activeModule === 'clipboard'"
          class="module-content module-content--fill"
        >
          <HudClipboardHistoryModule />
        </div>

        <!-- Screenshot History Module -->
        <div
          v-else-if="activeModule === 'screenshots'"
          class="module-content module-content--fill"
        >
          <HudScreenshotHistoryModule
            :screenshots-folder="config.screenshotsFolder"
          />
        </div>

        <!-- Transcript Module -->
        <div
          v-else-if="activeModule === 'transcript'"
          class="module-content module-content--fill"
        >
          <HudTranscriptModule />
        </div>

        <!-- Shortcuts Module -->
        <div
          v-else-if="activeModule === 'shortcuts'"
          class="module-content module-content--fill"
        >
          <HudShortcutsModule />
        </div>

        <!-- Fallback for unknown modules -->
        <div v-else class="module-content">
          <div class="card">
            <h2 style="margin-bottom: 16px">{{ activeModule }}</h2>
            <p style="color: var(--text-secondary); text-align: center">
              This module is coming soon.
            </p>
          </div>
        </div>
      </HudContentViewport>
    </div>

    <div v-if="isTop" class="trigger-zone trigger-top">
      <HudTriggerTab side="top" :tucked="isTucked" :pinned="isPinned"
        @hover="activationMode === 'hover' ? onMouseEnter() : undefined"
        @activate="activationMode === 'click' ? onTriggerClick() : undefined" />
    </div>

    <!-- Trigger zone (right for left-position) -->
    <div
      v-if="windowPosition === 'left'"
      class="trigger-zone trigger-right"
    >
      <HudTriggerTab
        side="right"
        :tucked="isTucked"
        :pinned="isPinned"
        @hover="activationMode === 'hover' ? onMouseEnter() : undefined"
        @activate="activationMode === 'click' ? onTriggerClick() : undefined"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { PanelsTopLeft, LayoutGrid, ArrowUpRight, Pin, Settings2, Monitor, MousePointer2, Mic, AudioLines, ChartNoAxesCombined, Info, ChevronRight } from 'lucide-vue-next'
import { horizontalWheel } from '#hud/utils/horizontalScroll'
import type { WindowPosition } from '#hud/composables/useConfig'
import { useCalculator } from '#hud/composables/useCalculator'
import { useConfig, speechLanguageCode, DEFAULT_SPEECH_HOTKEY } from '#hud/composables/useConfig'
import { useFibExtractor } from '#hud/composables/useFibExtractor'
import { useTranscript } from '#hud/composables/useTranscript'
definePageMeta({ layout: 'hud' })

const runtimeConfig = useRuntimeConfig();
const appVersion = runtimeConfig.public.appVersion;

const { inputs, levels, isLong, results, error, calculate, clear } =
  useCalculator();
const {
  fibPrices,
  isProcessing,
  status,
  scanRegion,
  captureAndExtract,
  getLevelPrices,
  clearFibPrices,
} = useFibExtractor();
const {
  config,
  loadConfig,
  setCalcSettings,
  setScanRegion,
  setWindowPosition,
  setColorTheme,
  setActivationMode,
  setMonitorIndex,
  setDisplayMode,
  setScreenshotsFolder,
  setSpeechLanguage,
  setSpeechModel,
  setSpeechDevice,
  setSpeechInsert,
  setSpeechHotkey,
  setSpeechLivePreview,
  setSpeechInputDevice,
  setAiProvider,
  setAiBaseUrl,
  setAiModel,
} = useConfig();

// QuantVoice: registered here, for the whole window, so a hotkey take lands in
// the transcript list even while another module is open.
const {
  status: speechStatus,
  devices: speechDevices,
  loadDevices: loadSpeechDevices,
  applySettings: applySpeechSettings,
} = useTranscript();

const SPEECH_LANGUAGES = [
  { code: "auto", label: "Auto-detect" },
  { code: "de", label: "Deutsch" },
  { code: "en", label: "English" },
  { code: "fr", label: "Français" },
  { code: "es", label: "Español" },
  { code: "it", label: "Italiano" },
  { code: "pt", label: "Português" },
  { code: "nl", label: "Nederlands" },
  { code: "pl", label: "Polski" },
  { code: "ru", label: "Русский" },
  { code: "uk", label: "Українська" },
  { code: "tr", label: "Türkçe" },
  { code: "sv", label: "Svenska" },
  { code: "da", label: "Dansk" },
  { code: "no", label: "Norsk" },
  { code: "fi", label: "Suomi" },
  { code: "cs", label: "Čeština" },
  { code: "hu", label: "Magyar" },
  { code: "ro", label: "Română" },
  { code: "el", label: "Ελληνικά" },
  { code: "ja", label: "日本語" },
  { code: "zh", label: "中文" },
  { code: "ko", label: "한국어" },
  { code: "ar", label: "العربية" },
  { code: "hi", label: "हिन्दी" },
];

const SPEECH_MODELS = [
  { id: "tiny", label: "tiny — 75 MB, fastest, rough" },
  { id: "base", label: "base — 145 MB, fast" },
  { id: "small", label: "small — 480 MB, good balance (default)" },
  { id: "medium", label: "medium — 1.5 GB, better, slow on CPU" },
  { id: "large-v3-turbo", label: "large-v3-turbo — 1.6 GB, best per second, GPU recommended" },
  { id: "large-v3", label: "large-v3 — 3 GB, most accurate, slowest" },
];

/** Device names once, WASAPI's when it lists them (MME truncates names). */
const micOptions = computed(() => {
  const all = speechDevices.value;
  const wasapi = all.filter((d) => d.hostapi.toLowerCase().includes("wasapi"));
  const pick = wasapi.length ? wasapi : all;
  return Array.from(new Set(pick.map((d) => d.name.trim()).filter(Boolean)));
});

async function onSpeechLanguage(v: string) {
  await setSpeechLanguage(v);
  await applySpeechSettings();
}
async function onSpeechModel(v: string) {
  await setSpeechModel(v);
  await applySpeechSettings();
}
async function onSpeechDevice(v: "auto" | "cpu" | "cuda") {
  await setSpeechDevice(v);
  await applySpeechSettings();
}
async function onSpeechInsert(v: "paste" | "type" | "clipboard") {
  await setSpeechInsert(v);
  await applySpeechSettings();
}
async function onSpeechHotkey(v: string) {
  await setSpeechHotkey(v);
  await applySpeechSettings();
}
async function onSpeechInputDevice(v: string) {
  await setSpeechInputDevice(v);
  await applySpeechSettings();
}
async function onSpeechLivePreview(v: boolean) {
  await setSpeechLivePreview(v);
  await applySpeechSettings();
}

const isPinned = ref(false);
const isTucked = ref(true);
let unlistenPosition: (() => void) | undefined;
let unlistenMonitor: (() => void) | undefined;
let unlistenRecover: (() => void) | null = null;
const activeModule = ref("home");
const settingsSection = ref('display');
const settingsTabs = [
  { id: 'display', label: 'Appearance', icon: Monitor },
  { id: 'behavior', label: 'Behavior', icon: MousePointer2 },
  { id: 'voice', label: 'Voice & language', icon: Mic },
  { id: 'controls', label: 'Recording', icon: AudioLines },
  { id: 'analysis', label: 'Chart analysis', icon: ChartNoAxesCombined },
  { id: 'about', label: 'About', icon: Info },
];
function scrollModuleNav(event: WheelEvent) {
  horizontalWheel(event, event.currentTarget as HTMLElement);
}
watch(activeModule, async () => {
  await nextTick();
  document.querySelector('.top-module-tab.active')?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
});
watch(settingsSection, async () => {
  await nextTick();
  document.querySelector('.settings-content')?.closest('.scroll-content')?.scrollTo({ left: 0, behavior: 'instant' });
});
const homeGeneralCollapsed = ref(false);
const homeAdvancedCollapsed = ref(false);

// Screenshots folder
const defaultScreenshotsFolder = ref("");

const displayMode = computed(() => config.value.displayMode || "basic");
const generalHomeIds = [
  "notes",
  "todos",
  "worldclock",
  "calendar",
  "gen-calc",
  "colorpicker",
  "clipboard",
  "screenshots",
  "transcript",
  "shortcuts",
];

const homeModules = [
  {
    id: "notes",
    label: "Notes",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></svg>',
  },
  {
    id: "todos",
    label: "Todo List",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 11l3 3L22 4"/><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/></svg>',
  },
  {
    id: "worldclock",
    label: "Clock",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>',
  },
  {
    id: "calendar",
    label: "Calendar",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="18" rx="2" ry="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>',
  },
  {
    id: "gen-calc",
    label: "Calculator",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="4" y="2" width="16" height="20" rx="2"/><line x1="8" y1="6" x2="16" y2="6"/><line x1="8" y1="10" x2="10" y2="10"/><line x1="14" y1="10" x2="16" y2="10"/><line x1="8" y1="14" x2="10" y2="14"/><line x1="14" y1="14" x2="16" y2="14"/><line x1="8" y1="18" x2="16" y2="18"/></svg>',
  },
  {
    id: "colorpicker",
    label: "Color Picker",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c1.1 0 2-.9 2-2v-.7c0-.5-.2-1-.5-1.3-.3-.3-.5-.8-.5-1.3 0-1.1.9-2 2-2h2.3c3 0 5.7-2.5 5.7-5.7C23 5.1 18.1 2 12 2z"/><circle cx="8" cy="10" r="1.5" fill="currentColor" stroke="none"/><circle cx="12" cy="7" r="1.5" fill="currentColor" stroke="none"/><circle cx="16" cy="10" r="1.5" fill="currentColor" stroke="none"/><circle cx="10" cy="14" r="1.5" fill="currentColor" stroke="none"/></svg>',
  },
  {
    id: "clipboard",
    label: "Clipboard",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><rect x="8" y="2" width="8" height="4" rx="1" ry="1"/></svg>',
  },
  {
    id: "screenshots",
    label: "Screenshots",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>',
  },
  {
    id: "transcript",
    label: "Transcript",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="22"/></svg>',
  },
  {
    id: "shortcuts",
    label: "Shortcuts",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>',
  },
  {
    id: "position-calc",
    label: "Position Sizer",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="4" y="2" width="16" height="20" rx="2"/><line x1="8" y1="6" x2="16" y2="6"/><line x1="8" y1="10" x2="16" y2="10"/><line x1="8" y1="14" x2="16" y2="14"/></svg>',
  },
  {
    id: "chart-analyzer",
    label: "Chart Analyzer",
    icon: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 3v18h18"/><path d="M7 16l4-8 4 6 4-4"/></svg>',
  },
];

const homeGeneralModules = computed(() =>
  homeModules.filter((m) => generalHomeIds.includes(m.id)),
);
const homeAdvancedModules = computed(() =>
  homeModules.filter((m) => !generalHomeIds.includes(m.id)),
);

const moduleDescriptions: Record<string, string> = {
  notes: 'Ideas, ready to write', todos: 'A little more done', worldclock: 'Time, timers & alarms', calendar: 'Your day at a glance',
  clipboard: 'Everything you copied', screenshots: 'Capture & revisit', transcript: 'Turn speech into text', colorpicker: 'Pick any screen color',
  'gen-calc': 'Calculate & convert', shortcuts: 'Your apps, one click away', 'position-calc': 'Plan your trade size', 'chart-analyzer': 'A closer look at charts',
};
const launcherGroups = computed(() => [
  { label: 'Organize', ids: ['notes', 'todos', 'calendar', 'worldclock'] },
  { label: 'Capture', ids: ['clipboard', 'screenshots', 'transcript', 'colorpicker'] },
  { label: 'Tools', ids: ['gen-calc', 'shortcuts', 'position-calc', 'chart-analyzer'] },
].map(group => ({ label: group.label, modules: group.ids.flatMap(id => topModules.value.filter(mod => mod.id === id)) })));

const windowLabel = ref("main");
const windowPosition = computed(() => {
  const pos = config.value.windowPosition || "left";
  if (pos === "dual") {
    return windowLabel.value === "dual-right" ? "right" : "left";
  }
  return pos;
});
const isTop = computed(() => windowPosition.value === 'top');
const topModules = computed(() => config.value.displayMode === 'pro' ? homeModules : homeGeneralModules.value);
const activationMode = computed(() => config.value.activationMode || "hover");
const availableMonitors = ref<
  Array<{
    index: number;
    name: string;
    width: number;
    height: number;
    is_primary: boolean;
  }>
>([]);
let invoke: any = null;

// Check if running in Tauri
const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// Load saved settings on mount
onMounted(async () => {
  // Start clipboard polling globally (survives module navigation)
  initClipboardPolling();
  // Same for the alarm checker — alarms fire from any module, not just the clock
  initWorldClock();

  await loadConfig();

  // Load default screenshots folder path
  if (isTauri) {
    try {
      const { invoke: inv } = await import("@tauri-apps/api/core");
      defaultScreenshotsFolder.value = await inv<string>("plugin:hud|get_default_screenshots_folder",
      );
    } catch (e) {
      console.warn("Failed to get default screenshots folder:", e);
    }
  }

  if (config.value.scanRegion) {
    scanRegion.value = config.value.scanRegion;
    status.value = `Region: ${config.value.scanRegion[2]}x${config.value.scanRegion[3]}`;
  }
  applyTheme();
  if (isTauri) {
    const core = await import("@tauri-apps/api/core");
    invoke = core.invoke;

    // Detect current window label (main vs dual-right)
    try {
      const { getCurrentWebviewWindow } =
        await import("@tauri-apps/api/webviewWindow");
      windowLabel.value = getCurrentWebviewWindow().label;
    } catch (e) {
      console.warn("Failed to get window label:", e);
    }

    // Listen for theme changes from the other window (dual mode)
    try {
      const { listen } = await import("@tauri-apps/api/event");
      await listen<string>("theme-changed", (event) => {
        const theme = event.payload as "light" | "dark";
        setColorTheme(theme);
        applyTheme();
      });
    } catch (e) {
      console.warn("Failed to listen for theme changes:", e);
    }

    // The crate asks for this after it un-minimised the window, after a
    // display change (RDP resolution, DPI, monitors) or a session reconnect
    // (resilience.rs). Re-place the window in whatever state it is in.
    try {
      const { getCurrentWebviewWindow } =
        await import("@tauri-apps/api/webviewWindow");
      unlistenRecover = await getCurrentWebviewWindow().listen<string>(
        "hud:recover",
        (event) => {
          void reapplyGeometry(event.payload);
        },
      );
    } catch (e) {
      console.warn("Failed to listen for window recovery:", e);
    }

    if (windowLabel.value !== 'dual-right') {
      const { listen } = await import('@tauri-apps/api/event');
      unlistenPosition = await listen<WindowPosition>('hud:position-change', event => {
        void handlePositionChange(event.payload);
      });
      unlistenMonitor = await listen<number>('hud:monitor-change', event => {
        void handleMonitorChange(event.payload);
      });
    }

    // Load available monitors
    try {
      availableMonitors.value = await invoke("plugin:hud|get_available_monitors");
    } catch (e) {
      console.warn("Failed to load monitors:", e);
      availableMonitors.value = [
        {
          index: 0,
          name: "Primary Monitor",
          width: 1920,
          height: 1080,
          is_primary: true,
        },
      ];
    }

    await invoke("plugin:hud|setup_window_size", {
      monitorIndex: config.value.monitorIndex,
    });

    // Determine effective tuck position
    const configPos = config.value.windowPosition || "left";
    const tuckPos =
      configPos === "dual"
        ? windowLabel.value === "dual-right"
          ? "right"
          : "left"
        : configPos;

    // Start tucked
    await invoke("plugin:hud|tuck_window", {
      position: tuckPos,
      monitorIndex: config.value.monitorIndex,
    });

    // In dual mode, the primary window must spawn the dual-right window on
    // startup. In the suite the primary overlay is labelled "hud" (it was
    // "main" in the standalone app), so key off "not dual-right" instead of a
    // hardcoded label.
    if (configPos === "dual" && windowLabel.value !== "dual-right") {
      try {
        await invoke("plugin:hud|create_dual_window", {
          monitorIndex: config.value.monitorIndex,
        });
      } catch (e) {
        console.warn("Failed to create dual window on startup:", e);
      }
    }

    try {
      const { register } = await import("@tauri-apps/plugin-global-shortcut");
      await register("F9", handleCapture);
    } catch (e) {
      console.warn("Failed to register hotkey:", e);
    }
  }
});

// Counterpart to the two global inits above — the overlay page owns them, so
// they end with it (dev HMR reload, or the window being torn down).
onUnmounted(() => {
  disposeClipboardPolling();
  disposeWorldClock();
  unlistenPosition?.();
  unlistenMonitor?.();
  unlistenRecover?.();
  unlistenRecover = null;
});

/**
 * Put the window back on its edge without changing what the user sees:
 * tucked stays tucked, expanded stays expanded. Both commands size and
 * position from the current monitor, so a stale geometry is corrected too.
 */
async function reapplyGeometry(why: string) {
  if (!isTauri || !invoke) return;
  try {
    if (isTucked.value) {
      await invoke("plugin:hud|tuck_window", {
        position: windowPosition.value,
        monitorIndex: config.value.monitorIndex,
      });
    } else {
      await invoke("plugin:hud|show_window", {
        position: windowPosition.value,
        monitorIndex: config.value.monitorIndex,
      });
    }
  } catch (e) {
    console.warn(`Failed to re-apply window geometry (${why}):`, e);
  }
}

async function onMouseEnter() {
  if (!isTucked.value) return;
  if (isTauri && invoke) {
    await invoke("plugin:hud|show_window", {
      position: windowPosition.value,
      monitorIndex: config.value.monitorIndex,
    });
  }
  isTucked.value = false;
}

async function onMouseLeave() {
  if (isTucked.value || isPinned.value) return;
  // In click mode, don't auto-tuck on mouse leave
  if (activationMode.value === "click") return;
  if (isTauri && invoke) {
    await invoke("plugin:hud|tuck_window", {
      position: windowPosition.value,
      monitorIndex: config.value.monitorIndex,
    });
  }
  isTucked.value = true;
}

async function onTriggerClick() {
  if (isTucked.value) {
    await onMouseEnter();
  } else {
    if (isPinned.value) return;
    if (isTauri && invoke) {
      await invoke("plugin:hud|tuck_window", {
        position: windowPosition.value,
        monitorIndex: config.value.monitorIndex,
      });
    }
    isTucked.value = true;
  }
}

async function togglePin() {
  isPinned.value = !isPinned.value;
  if (isPinned.value && isTauri && invoke) {
    await invoke("plugin:hud|show_window", {
      position: windowPosition.value,
      monitorIndex: config.value.monitorIndex,
    });
    isTucked.value = false;
  }
}

async function handleCapture() {
  await captureAndExtract();
  // Update levels from extracted fib prices
  const { entry, tp, sl } = getLevelPrices(isLong.value);
  if (entry) levels.entry = entry;
  if (tp) levels.tp = tp;
  if (sl) levels.sl = sl;
}

async function toggleRegion() {
  if (scanRegion.value) {
    scanRegion.value = null;
    setScanRegion(null);
    status.value = "Region cleared";
  } else {
    await selectRegion();
  }
}

async function selectRegion() {
  if (!isTauri || !invoke) return;

  // Open the region selector window
  await invoke("plugin:hud|open_region_selector");

  // Poll for result (window closes after selection)
  const checkResult = async () => {
    // `invoke` is declared `let invoke: any` in this file, and a type argument
    // on an untyped call is a TS error — annotate the binding instead.
    const region: [number, number, number, number] | null = await invoke(
      "plugin:hud|get_selected_region",
    );
    if (region) {
      scanRegion.value = region;
      setScanRegion(region);
      status.value = `Region: ${region[2]}x${region[3]}`;
    } else {
      // Check if selector window still exists
      const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      const selectorWindow = await WebviewWindow.getByLabel("region-selector");
      if (selectorWindow) {
        // Still selecting, check again
        setTimeout(checkResult, 100);
      } else {
        status.value = "Region selection cancelled";
      }
    }
  };

  // Start polling after a short delay
  setTimeout(checkResult, 200);
}

function setDirection(long: boolean) {
  isLong.value = long;
  // Refresh levels if we have fib prices
  if (Object.keys(fibPrices.value).length > 0) {
    const { entry, tp, sl } = getLevelPrices(long);
    if (entry) levels.entry = entry;
    if (tp) levels.tp = tp;
    if (sl) levels.sl = sl;
  }
}

function handleClear() {
  clear();
  clearFibPrices();
}

function updateLevels(newLevels: typeof levels) {
  Object.assign(levels, newLevels);
}

function updateInputs(newInputs: typeof inputs) {
  Object.assign(inputs, newInputs);
  setCalcSettings(newInputs);
}

async function copyToClipboard(value: string) {
  try {
    if (window.__TAURI__) {
      const { writeText } =
        await import("@tauri-apps/plugin-clipboard-manager");
      await writeText(value);
    } else {
      await navigator.clipboard.writeText(value);
    }
  } catch (e) {
    console.warn("Copy failed:", e);
  }
}

async function handlePositionChange(position: WindowPosition) {
  // The primary overlay owns position changes. A request from the right pane
  // must reach it before that pane is closed when leaving Dual mode.
  if (isTauri && windowLabel.value === 'dual-right') {
    const { emitTo } = await import('@tauri-apps/api/event');
    await emitTo('hud', 'hud:position-change', position);
    return;
  }
  setWindowPosition(position);
  if (isTauri && invoke) {
    // Always close any existing dual window first
    try {
      await invoke("plugin:hud|close_dual_window");
    } catch (_) {
      /* ignore if no dual window exists */
    }

    if (position === "dual") {
      // Position main window to left
      await invoke("plugin:hud|set_window_position", {
        position: "left",
        monitorIndex: config.value.monitorIndex,
      });
      // Create dual window on right
      await invoke("plugin:hud|create_dual_window", {
        monitorIndex: config.value.monitorIndex,
      });
    } else {
      await invoke("plugin:hud|set_window_position", {
        position,
        monitorIndex: config.value.monitorIndex,
      });
    }
  }
}

async function handleMonitorChange(event: Event | number) {
  const monitorIndex = typeof event === 'number' ? event : parseInt((event.target as HTMLSelectElement).value);
  if (isTauri && windowLabel.value === 'dual-right') {
    const { emitTo } = await import('@tauri-apps/api/event');
    await emitTo('hud', 'hud:monitor-change', monitorIndex);
    return;
  }
  setMonitorIndex(monitorIndex);

  if (isTauri && invoke) {
    await reapplyGeometry('monitor changed');

    // In dual mode, also move the dual-right window to the new monitor
    if (
      config.value.windowPosition === "dual" &&
      windowLabel.value !== "dual-right"
    ) {
      try {
        await invoke("plugin:hud|close_dual_window");
        await invoke("plugin:hud|create_dual_window", { monitorIndex });
      } catch (_) {
        /* ignore */
      }
    }
  }
}

async function handleThemeChange(theme: "light" | "dark") {
  setColorTheme(theme);
  applyTheme();
  // Broadcast to other windows (dual mode)
  if (isTauri) {
    try {
      const { emitTo } = await import("@tauri-apps/api/event");
      const target = windowLabel.value === "dual-right" ? "hud" : "dual-right";
      await emitTo(target, "theme-changed", theme);
    } catch (_) {
      /* ignore if target window doesn't exist */
    }
  }
}

function handleActivationModeChange(mode: "hover" | "click") {
  setActivationMode(mode);
  // If switching to click mode while untucked, stay untucked
  // If switching to hover mode while untucked, stay untucked
}

function handleDisplayModeChange(mode: "basic" | "pro") {
  setDisplayMode(mode);
}

function handleAiProviderChange(provider: "ollama" | "lmstudio") {
  setAiProvider(provider);
  // Set sensible default URL when switching providers
  if (provider === "ollama") {
    setAiBaseUrl("http://localhost:11434");
    if (!config.value.aiModel) setAiModel("llava");
  } else {
    setAiBaseUrl("http://localhost:1234");
  }
}

async function browseScreenshotsFolder() {
  try {
    const { invoke: inv } = await import("@tauri-apps/api/core");
    const currentPath =
      config.value.screenshotsFolder || defaultScreenshotsFolder.value || "";
    const selected = await inv<string | null>("plugin:hud|pick_folder", {
      defaultPath: currentPath || null,
    });
    if (selected) {
      setScreenshotsFolder(selected);
    }
  } catch (e) {
    console.warn("Failed to pick folder:", e);
  }
}

function applyTheme() {
  if (config.value.colorTheme) {
    document.documentElement.setAttribute(
      "data-theme",
      config.value.colorTheme,
    );
  }
}
</script>

<style scoped>
.app-wrapper {
  /* Was 100vh. The HUD does own its window, so viewport units would work here —
     but shell.css already gives html/body/#__nuxt a definite height, so 100%
     resolves identically and the module keeps the same rule as every other one.
     Nothing in the suite depends on the viewport. */
  height: 100%;
  width: 340px;
  display: flex;
  flex-direction: row;
  overflow: hidden;
}

/* Tucked state - window shrinks, hide main content */
.app-wrapper.tucked {
  width: 20px;
  pointer-events: none;
}
.app-wrapper.tucked .main-container {
  display: none;
}

.main-container {
  width: 320px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
}

/* Trigger zone: no layout space, just a positioning anchor */
.trigger-zone {
  position: relative;
  width: 20px;
  flex-shrink: 0;
  pointer-events: none;
}

.scroll-content {
  flex: 1;
  overflow-y: auto;
  padding: 2px 12px 8px;
  display: flex;
  flex-direction: column;
}

.header {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px 0;
  flex-shrink: 0;
}

.header-left,
.header-right {
  display: flex;
  align-items: center;
  gap: 4px;
}

.pin-btn.active {
  background: var(--accent-green-dim);
}

.title-logo {
  height: 22px;
  width: auto;
  object-fit: contain;
  user-select: none;
  -webkit-user-drag: none;
}

.capture-row {
  display: flex;
  justify-content: center;
  gap: 8px;
  margin: 6px 0;
}

.capture-btn {
  min-width: 140px;
}

.region-btn.active {
  background: var(--accent-green-dim);
  color: white;
}

.status {
  text-align: center;
  font-size: 13px;
  color: var(--text-secondary);
  margin: 3px 0;
}

.direction-toggle {
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 8px;
  margin: 6px 0;
}

.direction-btn {
  width: 120px;
  font-weight: 700;
}

.direction-btn.long.active {
  background: var(--accent-green-dim);
  color: white;
}

.direction-btn.short.active {
  background: var(--accent-red-dim);
  color: white;
}

.direction-btn:not(.active) {
  background: #444;
  color: var(--text-secondary);
}

.module-content {
  padding: 8px 0;
}

.module-content--fill {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.module-content .card {
  margin: 8px 0;
}

.module-content h2 {
  font-size: 18px;
  font-weight: 700;
  color: var(--text-primary);
  text-align: center;
}

.version-badge {
  text-align: center;
  font-size: 12px;
  color: var(--text-secondary);
  margin-bottom: 16px;
  padding: 6px 12px;
  background: var(--input-bg);
  border-radius: 6px;
  display: block;
}

.setting-group {
  margin-bottom: 16px;
}

.setting-label {
  display: block;
  font-size: 13px;
  color: var(--text-secondary);
  margin-bottom: 8px;
  font-weight: 500;
}

.setting-hint {
  font-size: 10.5px;
  line-height: 1.4;
  color: var(--text-secondary);
  margin-top: 4px;
}
.setting-hint.warn {
  color: #ffb400;
}
.setting-hint code {
  font-family: ui-monospace, Consolas, monospace;
  font-size: 10px;
  color: var(--text-primary);
}
.setting-checkbox {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-primary);
  cursor: pointer;
}

.monitor-select {
  width: 100%;
  padding: 10px 12px;
  background: var(--input-bg);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-primary);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.monitor-select:hover {
  border-color: var(--accent-blue);
  background: var(--bg-secondary);
}

.monitor-select:focus {
  outline: none;
  border-color: var(--accent-blue);
  background: var(--bg-secondary);
}

.monitor-select option {
  background: var(--bg-secondary);
  color: var(--text-primary);
}

.folder-picker {
  display: flex;
  gap: 6px;
  align-items: center;
}

.folder-path {
  flex: 1;
  min-width: 0;
  padding: 8px 10px;
  background: var(--input-bg);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-primary);
  font-size: 11px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.folder-browse-btn {
  flex-shrink: 0;
  padding: 8px 12px;
  font-size: 11px;
}

.ai-url-input {
  width: 100%;
  text-align: left;
  font-size: 13px;
}

/* Home Hub */
.home-hub {
  padding: 0;
}

.hub-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}

.hub-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 20px 12px;
  cursor: pointer;
  transition: all 0.15s ease;
  color: var(--text-secondary);
}

.hub-card:hover {
  border-color: var(--accent-blue);
  color: var(--text-primary);
  background: var(--bg-secondary);
}

.hub-icon {
  display: flex;
  align-items: center;
  justify-content: center;
}

.hub-label {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.3px;
}

.home-section-divider {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 12px 0 8px;
  cursor: pointer;
  user-select: none;
}

.home-divider-line {
  flex: 1;
  height: 1px;
  background: var(--border-color);
}

.home-divider-label {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  white-space: nowrap;
}

.home-section-divider:hover .home-divider-label {
  color: var(--text-primary);
}

.home-section-divider:hover .home-divider-line {
  background: var(--text-secondary);
}
</style>

<style src="../../assets/css/top-layout.css"></style>
