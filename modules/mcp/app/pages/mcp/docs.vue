<script setup lang="ts">
definePageMeta({ layout: 'mcp' })

const activeSection = ref('getting-started')

const sections = [
  { id: 'getting-started', label: 'Getting Started' },
  { id: 'adding-mcps', label: 'Adding MCPs' },
  { id: 'creating-tools', label: 'Creating Tools' },
  { id: 'ai-integration', label: 'AI Tool Integration' },
  { id: 'scripts', label: 'Internal Scripts' },
  { id: 'projects', label: 'Projects' },
  { id: 'codebase-indexing', label: 'Codebase Indexing' },
  { id: 'troubleshooting', label: 'Troubleshooting' },
]

function scrollTo(id: string) {
  document.getElementById(id)?.scrollIntoView({ behavior: 'smooth' })
}

let observer: IntersectionObserver | null = null

onMounted(() => {
  observer = new IntersectionObserver(
    (entries) => {
      // Pick the topmost visible section
      const visible = entries
        .filter(e => e.isIntersecting)
        .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)
      if (visible.length > 0) {
        activeSection.value = visible[0]!.target.id
      }
    },
    {
      // Trigger when a section enters the top 60% of the viewport
      rootMargin: '0px 0px -40% 0px',
      threshold: 0.1,
    }
  )
  sections.forEach(s => {
    const el = document.getElementById(s.id)
    if (el) observer!.observe(el)
  })
})

onUnmounted(() => {
  observer?.disconnect()
})
</script>

<template>
  <div class="docs-page">
    <div class="docs-layout">
      <nav class="docs-sidebar">
        <h3 class="docs-sidebar-title">Documentation</h3>
        <a
          v-for="s in sections"
          :key="s.id"
          class="docs-nav-item"
          :class="{ active: activeSection === s.id }"
          @click="scrollTo(s.id)"
        >
          {{ s.label }}
        </a>
      </nav>

      <div class="docs-content">
        <section id="getting-started" class="docs-section">
          <h1 class="docs-h1">Getting Started</h1>
          <p class="docs-text">
            QuantMCP is a desktop application for managing MCP (Model Context Protocol) servers.
            It provides a unified interface to configure, start, stop, and monitor MCP servers
            that integrate with AI tools like Claude Code, Cursor, and Windsurf.
          </p>
          <div class="docs-card">
            <h3 class="docs-card-title">Quick Overview</h3>
            <ul class="docs-list">
              <li><strong>Dashboard</strong> — View status of all MCPs, start/stop servers quickly</li>
              <li><strong>MCPs</strong> — Add, edit, and manage MCP server configurations</li>
              <li><strong>Tools</strong> — Create custom tools served by the built-in QuantMCP server</li>
              <li><strong>Scripts</strong> — Store and manage scripts internally without external files</li>
              <li><strong>Logs</strong> — Monitor input/output traffic for debugging</li>
              <li><strong>Settings</strong> — Configure application preferences</li>
            </ul>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">Built-in MCP Server</h3>
            <p class="docs-text">
              QuantMCP includes a built-in MCP server running on port <code>3100</code> by default.
              This server exposes any tools you create in the Tools page via the Streamable HTTP transport protocol.
              AI tools can connect to it at:
            </p>
            <pre class="docs-code">http://localhost:3100/mcp</pre>
          </div>
        </section>

        <section id="adding-mcps" class="docs-section">
          <h1 class="docs-h1">Adding MCPs</h1>
          <p class="docs-text">
            MCP servers can be added from the MCPs page. Each MCP entry represents an external
            MCP server that you want to manage through QuantMCP.
          </p>
          <div class="docs-card">
            <h3 class="docs-card-title">Transport Types</h3>
            <p class="docs-text"><strong>Stdio</strong> — The MCP server communicates via standard input/output. Best for local command-line tools.</p>
            <p class="docs-text"><strong>HTTP (Streamable HTTP)</strong> — The MCP server communicates via Streamable HTTP. Specify the port number.</p>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">Process Configuration</h3>
            <p class="docs-text">For MCPs that need to be launched as a process:</p>
            <ul class="docs-list">
              <li><strong>Command</strong> — The executable to run (e.g., <code>python</code>, <code>node</code>, <code>npx</code>)</li>
              <li><strong>Arguments</strong> — Command-line arguments, one per line</li>
              <li><strong>Environment Variables</strong> — Key-value pairs passed to the process</li>
              <li><strong>Working Directory</strong> — The directory to run the command from</li>
            </ul>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">Example: Adding an npx MCP</h3>
            <pre class="docs-code">Name: My File Server
Transport: Stdio
Command: npx
Arguments:
  -y
  @modelcontextprotocol/server-filesystem
  /path/to/allowed/dir</pre>
          </div>
        </section>

        <section id="creating-tools" class="docs-section">
          <h1 class="docs-h1">Creating Tools</h1>
          <p class="docs-text">
            Tools are custom functions served by the built-in QuantMCP MCP server. Each tool
            runs a script and receives input as JSON via stdin.
          </p>
          <div class="docs-card">
            <h3 class="docs-card-title">Tool Configuration</h3>
            <ul class="docs-list">
              <li><strong>Name</strong> — Unique identifier for the tool (e.g., <code>get_weather</code>)</li>
              <li><strong>Description</strong> — What the tool does (shown to AI models)</li>
              <li><strong>Script Path</strong> — Path to the script file, or use an internal script</li>
              <li><strong>Interpreter</strong> — Auto-detected from file extension, or specify manually</li>
              <li><strong>Input Parameters</strong> — Define the JSON schema for tool inputs</li>
            </ul>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">Example Script (Python)</h3>
            <pre class="docs-code">import json, sys

# Read input from stdin
data = json.loads(sys.stdin.read())
city = data.get("city", "Unknown")

# Return output to stdout
print(json.dumps({
    "temperature": "72°F",
    "city": city,
    "condition": "Sunny"
}))</pre>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">How It Works</h3>
            <ol class="docs-list docs-list-ordered">
              <li>AI tool sends a <code>tools/call</code> request to the QuantMCP server</li>
              <li>QuantMCP finds the matching tool definition</li>
              <li>The script is executed with the interpreter, input JSON piped to stdin</li>
              <li>Script stdout is captured and returned as the tool result</li>
            </ol>
          </div>
        </section>

        <section id="ai-integration" class="docs-section">
          <h1 class="docs-h1">AI Tool Integration</h1>
          <p class="docs-text">
            One button. <strong>Settings → Connect</strong> finds the AI tools installed on this
            machine and adds QuantMCP to each of them — Claude Code, Claude Desktop, Cursor,
            Windsurf, VS Code (Copilot), Cline, Roo Code, Kilo Code, Codex CLI, Gemini CLI,
            OpenCode, Continue, Amp, Aider. Nothing to pick: what is installed gets the entry,
            what is not is listed as not found. Press it again after a port change — every
            entry is rewritten.
          </p>
          <div class="docs-card">
            <h3 class="docs-card-title">What Connect writes</h3>
            <ul class="docs-list">
              <li><strong>CLI tools</strong> (Claude Code, Codex CLI, Gemini CLI, Kilo CLI) — their own <code>mcp add</code>, user scope</li>
              <li><strong>Config-file tools</strong> (Cursor, Windsurf, VS Code, Cline and its forks, OpenCode, Continue, Amp) — an entry in their server map, written atomically with the previous file kept as <code>.bak</code></li>
              <li><strong>Claude Desktop</strong> — through the <code>mcp-remote</code> stdio bridge, because its config takes no HTTP URL; Claude Desktop and Windsurf read their config at start, so the report says <em>restart</em></li>
              <li><strong>Aider</strong> — the URL in the <code>mcp-servers</code> list of <code>~/.aider.conf.yml</code></li>
            </ul>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">Any other MCP client</h3>
            <p class="docs-text">Plain MCP over Streamable HTTP — the snippet under the Connect button, copied where your client keeps its servers:</p>
            <pre class="docs-code">{
  "mcpServers": {
    "QuantMCP": {
      "type": "http",
      "url": "http://localhost:3100/mcp"
    }
  }
}</pre>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">Cloud clients (ChatGPT)</h3>
            <p class="docs-text">
              ChatGPT and other hosted clients reach MCP servers over public HTTPS only —
              <code>localhost</code> is out of their reach. Expose the URL through a tunnel first.
            </p>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">External MCPs</h3>
            <p class="docs-text">
              The same Connect sits on every MCP card (<strong>Config</strong>): it adds that
              server to every installed client, stdio entries in the shape each client expects.
            </p>
          </div>
        </section>

        <section id="scripts" class="docs-section">
          <h1 class="docs-h1">Internal Scripts</h1>
          <p class="docs-text">
            The Scripts page lets you store scripts directly inside QuantMCP, so you don't need
            to manage separate files on disk. Scripts are saved to the app data directory.
          </p>
          <div class="docs-card">
            <h3 class="docs-card-title">Supported Languages</h3>
            <ul class="docs-list">
              <li><strong>Python</strong> — <code>.py</code> files, run with <code>python</code></li>
              <li><strong>JavaScript</strong> — <code>.js</code> files, run with <code>node</code></li>
              <li><strong>Bash</strong> — <code>.sh</code> files, run with <code>bash</code></li>
              <li><strong>PowerShell</strong> — <code>.ps1</code> files, run with <code>powershell</code></li>
            </ul>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">Using with Tools</h3>
            <p class="docs-text">
              When creating a tool, you can choose "Internal Script" instead of providing
              an external file path. The tool will automatically use the stored script's path
              and set the correct interpreter.
            </p>
          </div>
        </section>

        <section id="projects" class="docs-section">
          <h1 class="docs-h1">Workspaces</h1>
          <p class="docs-text">
            QuantMCP works on the suite's workspaces — the one folder registry every module
            shares. A workspace is a registered project folder; open one anywhere in QuantSuite
            (or add one on the Workspaces page) and it carries a Kanban board, a code index,
            per-workspace memory and AGENT.md instructions, all keyed by the same registry entry.
          </p>
          <div class="docs-card">
            <h3 class="docs-card-title">The Registry</h3>
            <ul class="docs-list">
              <li><strong>One list</strong> — the same workspaces QuantSpace and QuantMemory see</li>
              <li><strong>Pinned first</strong> — pinned workspaces sort to the top, then by recency</li>
              <li><strong>Forget</strong> — removes the registry entry only; the folder and its kanban cards stay</li>
            </ul>
            <p class="docs-text" style="margin-top:10px">
              MCP tools take an optional <code>workspace</code> argument (name, entity id, or folder
              path); omitting it means the active workspace.
            </p>
            <p class="docs-text" style="margin-top:10px">
              The <strong>General</strong> entry's AgentOS holds the global <code>AGENT.md</code> — the
              brief every agent receives when it connects and again from <code>get_instructions</code>:
              how QuantMCP and its tools are used. A workspace's own <code>AGENT.md</code> is appended
              to it with project rules.
            </p>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">Kanban Boards</h3>
            <p class="docs-text">
              Each workspace has a Kanban board — <strong>Plan</strong>, <strong>Work</strong>,
              <strong>Review</strong> and <strong>Done</strong> — plus one suite-wide
              <strong>General</strong> board (the "General" entry here, and the board in the
              shell drawer). Add cards to track tasks, drag them between columns, and edit or
              delete as needed. Agents claim workspace cards through the MCP tools and work in
              isolated git worktrees under <code>.qs-worktrees/</code>; General cards have no
              folder, so they must be moved to a workspace board
              (<code>move_kanban_card_to_workspace</code>, or "→" on a drawer card) before an
              agent can claim them. The approval mode per board decides whether completed cards
              merge automatically or wait in Review. A claimed card's editor shows the test
              command that runs the app from its worktree — pick your shell (PowerShell, cmd or
              bash) and copy it; the agent gets the same commands from the tools.
            </p>
          </div>
        </section>

        <section id="codebase-indexing" class="docs-section">
          <h1 class="docs-h1">Codebase Indexing</h1>
          <p class="docs-text">
            Codebase indexing builds a searchable database of your project's source code.
            Once indexed, AI agents can query the index to find relevant functions, classes,
            and code patterns — without reading every file from scratch. Two indexing modes
            are available.
          </p>

          <div class="docs-card">
            <h3 class="docs-card-title">Structural Mode (BM25)</h3>
            <p class="docs-text">
              Structural indexing uses <strong>Tree-sitter</strong> to parse your source files into an
              AST (Abstract Syntax Tree) and extracts symbols: functions, classes, interfaces, imports,
              and variables. These symbols are stored in an <strong>SQLite FTS5</strong> full-text index
              and ranked using the <strong>BM25</strong> algorithm at query time.
            </p>
            <ul class="docs-list">
              <li>No AI model or network connection required</li>
              <li>Fast — indexes thousands of files in seconds</li>
              <li>Best for keyword-based searches like exact function or class names</li>
              <li>Supports 20+ languages: Python, JavaScript, TypeScript, Rust, Go, Java, C, C++, C#, Ruby, PHP, Swift, Kotlin, Scala, Lua, Bash, and more</li>
              <li>Falls back to regex-based extraction for unsupported languages</li>
            </ul>
            <p class="docs-text" style="margin-top:10px">
              <strong>When to use:</strong> You want instant, offline search without setting up an
              embedding model. Great for navigating large codebases by symbol name or keyword.
            </p>
          </div>

          <div class="docs-card">
            <h3 class="docs-card-title">Semantic Mode (Vector Embeddings)</h3>
            <p class="docs-text">
              Semantic indexing splits your source files into overlapping text chunks (~50 lines, ~10 line overlap),
              generates a vector embedding for each chunk using a <strong>local AI model</strong>, and stores the
              vectors in <strong>sqlite-vec</strong> for KNN (k-nearest neighbor) similarity search.
            </p>
            <ul class="docs-list">
              <li>Understands meaning — searches by concept, not just keywords</li>
              <li>Finds related code even if the exact terms don't match</li>
              <li>Requires a local embedding model running via <strong>Ollama</strong> or <strong>LM Studio</strong></li>
              <li>Slower to index (depends on model size and hardware)</li>
              <li>All data stays local — nothing is sent to the cloud</li>
            </ul>
            <p class="docs-text" style="margin-top:10px">
              <strong>When to use:</strong> You want natural-language code search — e.g., "function that handles
              user authentication" — or your queries are conceptual rather than exact keywords.
            </p>
          </div>

          <div class="docs-card">
            <h3 class="docs-card-title">Embedding Providers</h3>
            <p class="docs-text">
              Semantic mode requires a locally running embedding service. Two providers are supported:
            </p>
            <ul class="docs-list">
              <li><strong>Ollama</strong> — Default, runs at <code>http://localhost:11434</code>. Pull a model with <code>ollama pull nomic-embed-text</code></li>
              <li><strong>LM Studio</strong> — Runs at <code>http://localhost:1234</code>. Load an embedding model from the LM Studio UI, then enable the local server</li>
            </ul>
          </div>

          <div class="docs-card">
            <h3 class="docs-card-title">Recommended Embedding Models</h3>
            <p class="docs-text">
              All models below run locally on CPU. Choose based on your RAM, quality needs, and
              whether you need general-purpose or code-specific embeddings.
            </p>

            <h4 class="docs-h4">General Purpose (Ollama)</h4>
            <div class="docs-table-wrapper">
              <table class="docs-table">
                <thead>
                  <tr>
                    <th>Model</th>
                    <th>Sizes</th>
                    <th>Dims</th>
                    <th>Notes</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td><code>nomic-embed-text</code></td>
                    <td>274 MB</td>
                    <td>768</td>
                    <td>Best default — great balance of quality and speed, large context window</td>
                  </tr>
                  <tr>
                    <td><code>nomic-embed-text-v2-moe</code></td>
                    <td>~300 MB</td>
                    <td>768</td>
                    <td>Multilingual MoE variant, optimized for retrieval tasks</td>
                  </tr>
                  <tr>
                    <td><code>mxbai-embed-large</code></td>
                    <td>670 MB</td>
                    <td>1024</td>
                    <td>State-of-the-art from mixedbread.ai, outperforms some proprietary models</td>
                  </tr>
                  <tr>
                    <td><code>all-minilm</code></td>
                    <td>22 / 33 MB</td>
                    <td>384</td>
                    <td>Ultra-fast, tiny footprint — great for quick experiments or low-RAM machines</td>
                  </tr>
                  <tr>
                    <td><code>snowflake-arctic-embed</code></td>
                    <td>22 MB &ndash; 670 MB</td>
                    <td>384 &ndash; 1024</td>
                    <td>Suite of 5 sizes (22m to 335m params) — pick for your hardware. Strong retrieval</td>
                  </tr>
                  <tr>
                    <td><code>snowflake-arctic-embed2</code></td>
                    <td>~1.1 GB</td>
                    <td>1024</td>
                    <td>v2 with multilingual support, 100+ languages, up to 32K token context</td>
                  </tr>
                  <tr>
                    <td><code>bge-m3</code></td>
                    <td>1.2 GB</td>
                    <td>1024</td>
                    <td>Multilingual + multi-granularity, high quality, larger footprint</td>
                  </tr>
                  <tr>
                    <td><code>bge-large</code></td>
                    <td>670 MB</td>
                    <td>1024</td>
                    <td>BAAI English-focused embedding, strong benchmark scores</td>
                  </tr>
                  <tr>
                    <td><code>granite-embedding:30m</code></td>
                    <td>30 MB</td>
                    <td>384</td>
                    <td>IBM tiny model, good for constrained environments</td>
                  </tr>
                  <tr>
                    <td><code>granite-embedding:278m</code></td>
                    <td>278 MB</td>
                    <td>768</td>
                    <td>IBM dense biencoder, good for code</td>
                  </tr>
                  <tr>
                    <td><code>paraphrase-multilingual</code></td>
                    <td>~560 MB</td>
                    <td>768</td>
                    <td>Optimized for clustering and semantic search across 50+ languages</td>
                  </tr>
                  <tr>
                    <td><code>embeddinggemma</code></td>
                    <td>~600 MB</td>
                    <td>768</td>
                    <td>Google's 300M param embedding model</td>
                  </tr>
                  <tr>
                    <td><code>qwen3-embedding</code></td>
                    <td>0.6B / 4B / 8B</td>
                    <td>varies</td>
                    <td>#1 on MTEB multilingual leaderboard, 100+ languages — powerful but large</td>
                  </tr>
                </tbody>
              </table>
            </div>

            <h4 class="docs-h4" style="margin-top:16px">Code-Specialized Models</h4>
            <p class="docs-text">
              These models are specifically trained or optimized for code retrieval and understanding.
              They can be used via LM Studio or Hugging Face (load as an embedding model).
            </p>
            <div class="docs-table-wrapper">
              <table class="docs-table">
                <thead>
                  <tr>
                    <th>Model</th>
                    <th>Params</th>
                    <th>Dims</th>
                    <th>Notes</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td><code>nomic-embed-code</code></td>
                    <td>7B</td>
                    <td>4096</td>
                    <td>State-of-the-art code embedding, fully open-source, strong multilingual code support</td>
                  </tr>
                  <tr>
                    <td><code>CodeRankEmbed</code></td>
                    <td>137M</td>
                    <td>768</td>
                    <td>Optimized specifically for code search and retrieval tasks</td>
                  </tr>
                  <tr>
                    <td><code>jina-embeddings-v2-base-code</code></td>
                    <td>137M</td>
                    <td>768</td>
                    <td>Jina's code-optimized embeddings, fast inference, 30+ programming languages</td>
                  </tr>
                  <tr>
                    <td><code>CodeSage-large-v2</code></td>
                    <td>1.3B</td>
                    <td>flex</td>
                    <td>Three sizes (130M/356M/1.3B), Matryoshka flexible dimensions</td>
                  </tr>
                  <tr>
                    <td colspan="4" style="text-align:center;color:var(--text-muted);font-style:italic">
                      Code models may need to be loaded via LM Studio or downloaded from Hugging Face
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>

            <p class="docs-text" style="margin-top:12px">
              To pull an Ollama model: <code>ollama pull &lt;model-name&gt;</code><br />
              For LM Studio: search and download the model from the LM Studio UI, then start the local server.
            </p>
          </div>

          <div class="docs-card">
            <h3 class="docs-card-title">How Indexing Works</h3>
            <ol class="docs-list docs-list-ordered">
              <li>Select your workspace on the Workspaces page</li>
              <li>Choose a mode — <strong>Structural</strong> or <strong>Semantic</strong></li>
              <li>If semantic, configure the provider, model, and base URL</li>
              <li>Click <strong>Index</strong></li>
              <li>QuantMCP walks the codebase, skipping common non-code directories (<code>node_modules</code>, <code>.git</code>, <code>target</code>, etc.) and binary files</li>
              <li>Each file is hashed (SHA-256) — unchanged files are skipped on re-index</li>
              <li>A <code>.db</code> file is created at <code>~/.quantsuite/modules/mcp/indexes/&lt;workspace-id&gt;.db</code></li>
            </ol>
          </div>

          <div class="docs-card">
            <h3 class="docs-card-title">AI Integration</h3>
            <p class="docs-text">
              Codebase indexing is managed by QuantMCP. You only need one MCP connection:
              <strong>QuantMCP</strong> on <code>http://localhost:3100/mcp</code>.
            </p>
            <p class="docs-text">
              In Settings, press <strong>Connect</strong> — it adds QuantMCP to every AI tool
              installed on this machine — or copy the standard snippet from the same card.
            </p>
          </div>
        </section>

        <section id="troubleshooting" class="docs-section">
          <h1 class="docs-h1">Troubleshooting</h1>
          <div class="docs-card">
            <h3 class="docs-card-title">MCP Server Won't Start</h3>
            <ul class="docs-list">
              <li>Check that the command and arguments are correct</li>
              <li>Verify the working directory exists</li>
              <li>Ensure required environment variables are set</li>
              <li>Check the Logs tab for error messages</li>
            </ul>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">Tools Not Responding</h3>
            <ul class="docs-list">
              <li>Verify the script path exists and is executable</li>
              <li>Check that the interpreter is installed and on PATH</li>
              <li>Test the tool using the play button on the Tools page</li>
              <li>Check script output — it must write valid text to stdout</li>
            </ul>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">Port 3100 Already in Use</h3>
            <p class="docs-text">
              The built-in MCP server runs on port 3100. If another process is using this port,
              close that process or change the port in Settings.
            </p>
          </div>
          <div class="docs-card">
            <h3 class="docs-card-title">AI Tool Can't Connect</h3>
            <ul class="docs-list">
              <li>Make sure QuantMCP is running</li>
              <li>Verify the URL is <code>http://localhost:3100/mcp</code></li>
              <li>Check firewall settings aren't blocking local connections</li>
              <li>Press <strong>Connect</strong> in Settings again — it rewrites every entry</li>
            </ul>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
.docs-page { width: 100%; height: 100%; }

.docs-layout {
  display: flex;
  gap: 24px;
  height: 100%;
}

.docs-sidebar {
  width: 180px;
  flex-shrink: 0;
  position: sticky;
  top: 0;
  align-self: flex-start;
}

.docs-sidebar-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 12px;
}

.docs-nav-item {
  display: block;
  padding: 6px 12px;
  font-size: 13px;
  color: var(--text-muted);
  border-radius: var(--radius);
  cursor: pointer;
  transition: all var(--transition);
  text-decoration: none;
  margin-bottom: 2px;
}

.docs-nav-item:hover { color: var(--text-primary); background: var(--bg-hover); }
.docs-nav-item.active { color: var(--accent); background: var(--bg-hover); }

.docs-content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding-bottom: 60px;
}

.docs-section { margin-bottom: 40px; }

.docs-h1 {
  font-size: 22px;
  font-weight: 600;
  margin-bottom: 12px;
}

.docs-text {
  font-size: 14px;
  line-height: 1.7;
  color: var(--text-secondary);
  margin-bottom: 12px;
}

.docs-text code {
  background: var(--bg-hover);
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 13px;
  color: var(--accent);
}

.docs-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 20px;
  margin-bottom: 12px;
}

.docs-card-title {
  font-size: 15px;
  font-weight: 600;
  margin-bottom: 10px;
}

.docs-h4 {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  margin-bottom: 8px;
}

.docs-list {
  list-style: disc;
  padding-left: 20px;
  font-size: 14px;
  line-height: 1.8;
  color: var(--text-secondary);
}

.docs-list-ordered { list-style: decimal; }

.docs-list code {
  background: var(--bg-hover);
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 13px;
  color: var(--accent);
}

.docs-code {
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 14px 16px;
  font-size: 13px;
  font-family: monospace;
  color: var(--text-primary);
  overflow-x: auto;
  line-height: 1.6;
  margin-top: 8px;
  white-space: pre;
}

.docs-table-wrapper {
  overflow-x: auto;
  margin-top: 8px;
}

.docs-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
  color: var(--text-secondary);
}

.docs-table th {
  text-align: left;
  font-weight: 600;
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-muted);
  padding: 8px 12px;
  border-bottom: 2px solid var(--border);
  white-space: nowrap;
}

.docs-table td {
  padding: 8px 12px;
  border-bottom: 1px solid var(--border);
  vertical-align: top;
}

.docs-table tr:last-child td {
  border-bottom: none;
}

.docs-table code {
  background: var(--bg-hover);
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 12px;
  color: var(--accent);
  white-space: nowrap;
}
</style>
