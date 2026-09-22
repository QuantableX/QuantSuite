// Exercise the actual Notes/Specs components with isolated IPC and Vue state.
// No native app, user files or settings are touched.
import assert from 'node:assert/strict'
import { test, after } from 'node:test'
import { readFile, writeFile, mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { parse, compileScript } from '@vue/compiler-sfc'

const root = fileURLToPath(new URL('../', import.meta.url))
const mockSource = `
import { reactive } from 'vue';
export const ws = reactive({ contentWorkspace: { id: 'a', name: 'A', path: 'C:/projects/a' }, contentWorkspaceId: 'a' });
export const app = reactive({ activeEditorTabs: [], editorVisible: true, lastDeletedFile: null, lastSavedFile: null,
  openTab(path, content) { this.activeEditorTabs.push({filePath:path,content}); },
  pathsMatch(a,b) { return a === b; }, triggerFileExplorerRefresh() {}, toggleNotesBar() {},
  notifyFileSaved() {}, refreshTab() {}, notifyFileDeleted(path) { this.lastDeletedFile={filePath:path}; },
});
export const useWorkspacesStore = () => ws;
export const useAppStore = () => app;
export const mock = { files: new Map(), calls: [], hold: null, failStorage: false };
export async function invoke(command, args) {
  const name = command.split('|')[1]; mock.calls.push({name,...args});
  if (name === 'workspace_storage_dir') {
    if(mock.failStorage) throw Error('Migration could not preserve original data');
    if(mock.hold?.folder === args.folderPath) await mock.hold.promise;
    return 'C:/private/' + args.folderPath.split('/').at(-1) + '/canvas';
  }
  if (name === 'read_file') { if(!mock.files.has(args.path)) throw 'File does not exist: ' + args.path; return mock.files.get(args.path); }
  if (name === 'write_file') { mock.files.set(args.path,args.content); return; }
  if (name === 'delete_file') { mock.files.delete(args.path); return; }
  if (name === 'read_dir_tree') return [...mock.files.keys()].filter(p=>p.startsWith(args.path+'/')).map(path=>({path,name:path.split('/').at(-1),isDirectory:false}));
  throw Error('Unexpected IPC ' + name);
}
`
const plugin = { name: 'storage-fixture', setup(b) {
  b.onResolve({ filter: /^(mock|@tauri-apps\/api\/core)$/ }, () => ({ path: 'mock', namespace: 'fixture' }))
  b.onResolve({ filter: /stores\/(app|workspaces)$/ }, () => ({ path: 'mock', namespace: 'fixture' }))
  b.onLoad({ filter: /.*/, namespace: 'fixture' }, () => ({ contents: mockSource, loader: 'js', resolveDir: root }))
  b.onLoad({ filter: /\.vue$/ }, async ({ path }) => {
    const { descriptor } = parse(await readFile(path, 'utf8'), { filename: path })
    const compiled = compileScript(descriptor, { id: 'storage-test' })
    return { contents: `import {ref,computed,watch,onMounted,onUnmounted,nextTick} from 'vue';\n${compiled.content.replace('export default ', 'const component = ')}\ncomponent.render=()=>null; export default component;`, loader: 'ts', resolveDir: dirname(path) }
  })
} }
const bundle = (await build({
  stdin: { contents: `
    import {createRenderer,nextTick} from 'vue';
    import Notes from './modules/canvas/app/components/UI/NotesBar.vue';
    import Specs from './modules/canvas/app/components/Windows/SpecWindow.vue';
    export {mock,ws,app} from 'mock'; export {nextTick};
    const renderer=createRenderer({createComment:()=>({}),createText:()=>({}),createElement:()=>({}),insert(){},remove(){},setElementText(){},setText(){},patchProp(){},parentNode(){},nextSibling(){}});
    export function mount(kind) {const a=renderer.createApp(kind==='notes'?Notes:Specs,{window:{}});const vm=a.mount({});return {state:vm.$.setupState,stop:()=>a.unmount()};}
  `, resolveDir: root }, plugins: [plugin], bundle: true, write: false, format: 'esm', platform: 'node',
  define: { 'process.env.NODE_ENV': '"production"' },
})).outputFiles[0].text
const scratch=await mkdtemp(join(tmpdir(),'qs-workspace-storage-tests-'))
after(()=>rm(scratch,{recursive:true,force:true}))
let sequence = 0
async function fixture() {
  globalThis.window = {addEventListener(){},removeEventListener(){}}
  const file=join(scratch,`fixture-${sequence++}.mjs`)
  await writeFile(file,bundle)
  return import(pathToFileURL(file).href)
}
async function flush(m) { await m.nextTick(); await new Promise(resolve=>setTimeout(resolve,0)); await m.nextTick() }
const specText = `---\ntitle: Existing spec\nstatus: open\npriority: medium\ncreatedAt: 2026-09-22T00:00:00Z\nupdatedAt: 2026-09-22T00:00:00Z\nlinkedFiles:\n---\nKeep this content\n`

test('notes load, create and save use private workspace paths', async () => {
  const m = await fixture()
  m.mock.files.set('C:/private/a/canvas/NOTES.md', 'existing notes')
  const view = m.mount('notes'); await flush(m)
  assert.equal(view.state.content, 'existing notes')
  m.ws.contentWorkspace = {id:'b',name:'B',path:'C:/projects/b'}; m.ws.contentWorkspaceId='b'; await flush(m)
  assert.equal(view.state.fileExists, false)
  await view.state.createNotesFile()
  view.state.content='new B notes'; await view.state.saveNotes()
  assert.equal(m.mock.files.get('C:/private/b/canvas/NOTES.md'), 'new B notes')
  assert.equal(m.mock.files.get('C:/private/a/canvas/NOTES.md'), 'existing notes')
  assert.ok(m.mock.calls.filter(c=>c.path).every(c=>c.path.startsWith('C:/private/')))
  view.stop()
})

test('a slow prior workspace cannot replace the newly selected notes', async () => {
  const m = await fixture(); let finish
  m.mock.hold={folder:'C:/projects/a',promise:new Promise(resolve=>{finish=resolve})}
  m.mock.files.set('C:/private/a/canvas/NOTES.md','A'); m.mock.files.set('C:/private/b/canvas/NOTES.md','B')
  const view=m.mount('notes'); await flush(m)
  m.ws.contentWorkspace={id:'b',name:'B',path:'C:/projects/b'}; m.ws.contentWorkspaceId='b'; await flush(m)
  finish(); await flush(m)
  assert.equal(view.state.content,'B'); assert.equal(view.state.notesPath,'C:/private/b/canvas/NOTES.md')
  view.stop()
})

test('storage failures remain visible and do not offer an overwrite of old notes', async () => {
  const m=await fixture(); m.mock.failStorage=true
  const view=m.mount('notes'); await flush(m)
  assert.match(view.state.loadError,/preserve original/)
  assert.equal(view.state.notesPath,null)
  await view.state.createNotesFile()
  assert.equal(m.mock.calls.filter(c=>c.name==='write_file').length,0)
  view.stop()
})

test('specs retain content through internal load, editor open, status, create and delete', async () => {
  const m=await fixture(); const path='C:/private/a/canvas/specs/existing.spec.md'
  m.mock.files.set(path,specText)
  const view=m.mount('specs'); await flush(m)
  assert.equal(view.state.specs.length,1)
  const spec=view.state.specs[0]; assert.equal(spec.path,path)
  await view.state.openInEditor(spec); assert.equal(m.app.activeEditorTabs[0].filePath,path)
  await view.state.changeStatus(spec,'done'); assert.match(m.mock.files.get(path),/status: done/); assert.match(m.mock.files.get(path),/Keep this content/)
  view.state.newTitle='New task'; await view.state.createSpec()
  assert.ok(m.mock.files.has('C:/private/a/canvas/specs/new-task.spec.md'))
  view.state.requestDeleteSpec(spec); await view.state.handleDelete(true); assert.equal(m.mock.files.has(path),false)
  m.ws.contentWorkspace={id:'b',name:'B',path:'C:/projects/b'};m.ws.contentWorkspaceId='b';await flush(m)
  assert.equal(view.state.specs.length,0)
  assert.ok(m.mock.calls.filter(c=>c.path).every(c=>c.path.startsWith('C:/private/')))
  view.stop()
})
