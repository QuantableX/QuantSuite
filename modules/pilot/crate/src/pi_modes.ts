// QuantPilot's permission modes for pi and omp (card 20), loaded with
// `-e <this file>`. Pi has no permission system of its own: this extension
// stops tool calls the thread's mode does not allow, and asks through the
// RPC extension UI (a `select` titled "QP_APPROVAL {json}") where the user
// decides. QuantPilot switches the mode with the `/qp-mode <mode>` command.
//   full  — everything runs
//   edits — reading and editing run; commands ask
//   ask   — reading runs; edits and commands ask
//   plan  — reading only; anything else is declined
const READ = new Set(['read', 'ls', 'find', 'grep', 'glob', 'search', 'web_search', 'web_fetch', 'fetch', 'todo', 'todo_write', 'think'])
const EDIT = new Set(['write', 'edit', 'multi_edit', 'apply_patch', 'patch', 'notebook_edit'])

export default function (pi: any) {
  let mode: string = process.env.QP_PILOT_MODE || 'ask'
  const always = new Set<string>()

  pi.registerCommand('qp-mode', {
    description: "QuantPilot's permission mode for this session",
    handler: async (args: string) => {
      const next = String(args || '').trim()
      if (['full', 'edits', 'ask', 'plan'].includes(next)) mode = next
    },
  })

  pi.on('tool_call', async (event: any, ctx: any) => {
    const tool = String(event.toolName || '')
    if (mode === 'full' || READ.has(tool)) return undefined
    if (mode === 'plan') return { block: true, reason: 'Plan mode is read-only: describe the change instead of making it.' }
    if (mode === 'edits' && EDIT.has(tool)) return undefined
    if (always.has(tool)) return undefined
    const choice = await ctx.ui.select(`QP_APPROVAL ${JSON.stringify({ tool, input: event.input, id: event.toolCallId })}`, ['Allow', 'Allow for this session', 'Deny'])
    if (choice === 'Allow for this session') {
      always.add(tool)
      return undefined
    }
    if (choice === 'Allow') return undefined
    return { block: true, reason: 'The user declined this.' }
  })
}
