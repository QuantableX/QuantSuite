/**
 * Design artboards (round 3, card 06): the mockup's frame document — a CSP
 * that lets nothing load from the network, and a small bridge that picks an
 * element (hover outline, click → a CSS selector) and reports where pinned
 * elements are — plus the device widths and the message an agent gets for a
 * comment. The frame is sandboxed without same-origin: postMessage is its
 * only way out.
 */
import type { ArtboardDevice } from '../../shared/types'

export const DEVICES: { id: ArtboardDevice; label: string; width: number }[] = [
  { id: 'desktop', label: 'Desktop', width: 1280 },
  { id: 'tablet', label: 'Tablet', width: 820 },
  { id: 'phone', label: 'Phone', width: 390 },
]

export const CSP = "default-src 'none'; img-src data: blob:; media-src data: blob:; font-src data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'"

/** Runs inside the mockup. Messages carry `qcArt: 1`. */
const BRIDGE = `(function(){
var picking=false,watched=[],box=null;
function ensureBox(){if(box)return box;box=document.createElement('div');box.setAttribute('data-qc-art','');box.style.cssText='position:fixed;pointer-events:none;border:2px solid #7aa2f7;background:rgba(122,162,247,.14);border-radius:2px;z-index:2147483647;display:none';(document.body||document.documentElement).appendChild(box);return box}
function ident(s){return /^[A-Za-z][A-Za-z0-9_-]*$/.test(s)}
function sel(el){if(el.id&&ident(el.id))return '#'+el.id;var parts=[];while(el&&el.nodeType===1&&el!==document.documentElement){var p=el.parentElement;if(!p)break;var tag=el.tagName.toLowerCase();var same=[].filter.call(p.children,function(c){return c.tagName===el.tagName});parts.unshift(same.length>1?tag+':nth-of-type('+(same.indexOf(el)+1)+')':tag);if(p.id&&ident(p.id)){parts.unshift('#'+p.id);break}el=p}return parts.join(' > ')}
function rects(){return watched.map(function(s){try{var e=document.querySelector(s);if(!e)return null;var r=e.getBoundingClientRect();return{x:r.left,y:r.top,w:r.width,h:r.height}}catch(_){return null}})}
function report(){parent.postMessage({qcArt:1,type:'rects',rects:rects()},'*')}
function stop(){picking=false;if(box)box.style.display='none';document.documentElement.style.cursor=''}
addEventListener('message',function(e){var d=e.data||{};if(d.qcArt!==1)return;if(d.type==='pick'){if(d.on){picking=true;ensureBox();document.documentElement.style.cursor='crosshair'}else stop()}if(d.type==='watch'){watched=d.selectors||[];report()}});
document.addEventListener('mousemove',function(e){if(!picking)return;var t=e.target;if(!t||t===box||t===document.documentElement)return;var r=t.getBoundingClientRect(),b=ensureBox();b.style.display='block';b.style.left=r.left+'px';b.style.top=r.top+'px';b.style.width=r.width+'px';b.style.height=r.height+'px'},true);
document.addEventListener('click',function(e){if(!picking)return;e.preventDefault();e.stopPropagation();var t=e.target;if(!t||t===document.documentElement)t=document.body;stop();parent.postMessage({qcArt:1,type:'picked',selector:sel(t),quote:String(t.innerText||t.getAttribute('alt')||t.getAttribute('aria-label')||'').trim().slice(0,160)},'*')},true);
document.addEventListener('keydown',function(e){if(picking&&e.key==='Escape'){stop();parent.postMessage({qcArt:1,type:'cancel'},'*')}},true);
addEventListener('scroll',report,true);addEventListener('resize',report);addEventListener('load',function(){report();parent.postMessage({qcArt:1,type:'ready'},'*')});
try{new ResizeObserver(report).observe(document.documentElement)}catch(_){}
})();`

/** The frame document: CSP first, the bridge, then the mockup as written. */
export function frameDoc(html: string): string {
  const body = html.replace(/^\s*<!doctype[^>]*>/i, '')
  return `<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${CSP}"><script>${BRIDGE}<\/script>${body}`
}

/** What "Ask the agent" sends for a comment. */
export function askText(a: { id: string; name: string }, c: { id: string; selector: string; quote: string; text: string }): string {
  const quote = c.quote ? ` — “${c.quote.length > 80 ? `${c.quote.slice(0, 80)}…` : c.quote}”` : ''
  return [
    `Design comment on the artboard “${a.name}” (id \`${a.id}\`, in this workspace's QuantCanvas):`,
    `- Element: \`${c.selector}\`${quote}`,
    `- Comment: ${c.text}`,
    '',
    `Read the mockup with quantsuite.canvas.design_read, change it with quantsuite.canvas.design_write (artboard \`${a.id}\`), then mark the comment resolved with quantsuite.canvas.design_resolve (comment \`${c.id}\`) and a short note.`,
  ].join('\n')
}

/** The mockup a new artboard starts with. */
export function starterHtml(name: string): string {
  const safe = name.replace(/[<>&]/g, '')
  return `<!doctype html>
<html><head><meta charset="utf-8"><title>${safe}</title>
<style>body{margin:0;font-family:system-ui,sans-serif;color:#1d1d22;background:#fff}main{max-width:960px;margin:0 auto;padding:64px 32px}h1{font-size:40px;margin:0 0 12px}p{color:#5b5b66;font-size:18px}</style>
</head><body><main><h1>${safe}</h1><p>An empty artboard. Ask an agent to design it, or pin a comment to say what it should become.</p></main></body></html>`
}
