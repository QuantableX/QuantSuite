"""English-only offline research reports: one shared design, one template per report.

Every report is a single self-contained HTML file: inline CSS and script, the
evidence embedded as JSON, no network. The app shows it in a sandboxed iframe;
the same file opens in any browser and follows the OS light/dark setting.
"""
import html
import json


def payload(value):
    """JSON that cannot close the script element it is embedded in."""
    return (json.dumps(value, ensure_ascii=True, allow_nan=False, separators=(",", ":"))
            .replace("<", "\\u003c").replace(">", "\\u003e").replace("&", "\\u0026"))


def page(title, body, script, data):
    """The shared document: design tokens, components and helpers, then the report."""
    return (DOCUMENT.replace("__TITLE__", html.escape(title)).replace("__CSS__", BASE_CSS)
            .replace("__BODY__", body).replace("__BASE_JS__", BASE_JS)
            .replace("__SCRIPT__", script).replace("__DATA__", payload(data)))


def render(run, results):
    kind = results.get("kind", run.get("kind"))
    if kind in ("gauntlet", "gauntlet_overview"):
        from .lab_report_gauntlet import render as gauntlet
        return gauntlet(run, results)
    if kind == "profit" and results.get("format") == 2:
        from .lab_report_profit import render as profit
        return profit(run, results)
    if kind == "aggregates":
        from .lab_report_aggregates import render as aggregates
        return aggregates(run, results)
    # Runs written before the report templates keep their original page.
    data = json.dumps({"run": run, **results}, ensure_ascii=True, allow_nan=False).replace("<", "\\u003c")
    return LEGACY_TEMPLATE.replace("__TITLE__", html.escape(run["title"])).replace("__DATA__", data)


DOCUMENT = r'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="color-scheme" content="light dark"><title>__TITLE__</title><style>__CSS__</style></head>
<body><div class="page">__BODY__</div>
<div class="tooltip" id="tooltip" role="status" hidden></div>
<dialog class="sheet" id="sheet" aria-labelledby="sheet-title"><div class="sheet-head"><div><div class="kicker" id="sheet-kicker"></div><h2 id="sheet-title"></h2><div class="sheet-sub" id="sheet-sub"></div></div><button class="btn" id="sheet-close" type="button">Close</button></div><div class="sheet-body" id="sheet-body"></div></dialog>
<script>const DATA=__DATA__;
__BASE_JS__
__SCRIPT__</script></body></html>'''


# Colors: the validated reference palette (categorical order, status, diverging
# blue/red with a gray midpoint), stepped separately for the dark surface.
BASE_CSS = r'''
:root{color-scheme:light;--page:#f6f6f3;--surface:#fcfcfb;--surface-2:#f1f1ee;--line:#e1e0d9;--axis:#c3c2b7;--ink:#0b0b0b;--ink-2:#52514e;--muted:#6f6e69;--accent:#2a78d6;--accent-wash:rgba(42,120,214,.10);--ring:rgba(11,11,11,.10);--good:#0ca30c;--good-ink:#006300;--warn:#fab219;--warn-ink:#7a4f00;--bad:#d03b3b;--bad-ink:#b3302f;--mid:#f0efec;--pos:#2a78d6;--neg:#d03b3b;--s1:#2a78d6;--s2:#eb6834;--s3:#1baf7a;--s4:#eda100;--s5:#e87ba4;--s6:#008300;--s7:#6250d6;--s8:#e34948;--base:#52514e;--shadow:0 1px 2px rgba(11,11,11,.04),0 10px 30px rgba(11,11,11,.06)}
@media (prefers-color-scheme:dark){:root:where(:not([data-theme="light"])){color-scheme:dark;--page:#111110;--surface:#1a1a19;--surface-2:#222221;--line:#2c2c2a;--axis:#383835;--ink:#f0efec;--ink-2:#c3c2b7;--muted:#9a9890;--accent:#3987e5;--accent-wash:rgba(57,135,229,.16);--ring:rgba(255,255,255,.10);--good:#0ca30c;--good-ink:#2fc22f;--warn:#fab219;--warn-ink:#fab219;--bad:#d03b3b;--bad-ink:#ee7d7d;--mid:#383835;--pos:#3987e5;--neg:#e66767;--s1:#3987e5;--s2:#d95926;--s3:#199e70;--s4:#c98500;--s5:#d55181;--s6:#008300;--s7:#9085e9;--s8:#e66767;--base:#c3c2b7;--shadow:none}}
:root[data-theme="dark"]{color-scheme:dark;--page:#111110;--surface:#1a1a19;--surface-2:#222221;--line:#2c2c2a;--axis:#383835;--ink:#f0efec;--ink-2:#c3c2b7;--muted:#9a9890;--accent:#3987e5;--accent-wash:rgba(57,135,229,.16);--ring:rgba(255,255,255,.10);--good:#0ca30c;--good-ink:#2fc22f;--warn:#fab219;--warn-ink:#fab219;--bad:#d03b3b;--bad-ink:#ee7d7d;--mid:#383835;--pos:#3987e5;--neg:#e66767;--s1:#3987e5;--s2:#d95926;--s3:#199e70;--s4:#c98500;--s5:#d55181;--s6:#008300;--s7:#9085e9;--s8:#e66767;--base:#c3c2b7;--shadow:none}
*{box-sizing:border-box}html{-webkit-text-size-adjust:100%;scroll-behavior:smooth;scroll-padding-top:72px}
body{margin:0;background:var(--page);color:var(--ink);font:14px/1.55 system-ui,-apple-system,"Segoe UI",sans-serif}
button,input,select{font:inherit;color:inherit}
a{color:var(--accent)}
:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.page{max-width:1280px;margin:0 auto;padding:36px 32px 56px}
.eyebrow{font-size:12px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--ink-2)}
h1{font-size:clamp(26px,3.2vw,36px);line-height:1.15;letter-spacing:-.02em;font-weight:650;margin:8px 0 10px;max-width:920px}
h2{font-size:20px;line-height:1.25;letter-spacing:-.01em;font-weight:620;margin:0}
h3{font-size:15px;font-weight:620;margin:0 0 8px}
p{margin:0 0 10px}
.lede{color:var(--ink-2);font-size:15px;max-width:860px;margin:0}
.chips{display:flex;flex-wrap:wrap;gap:8px;margin-top:16px}
.chip{display:inline-flex;align-items:center;gap:6px;border:1px solid var(--line);background:var(--surface);border-radius:999px;padding:4px 11px;font-size:12px;color:var(--ink-2);white-space:nowrap}
.mono,.key{font-family:ui-monospace,"Cascadia Mono",Consolas,monospace}
.key{font-size:11.5px;color:var(--muted);overflow-wrap:anywhere}
.muted{color:var(--ink-2)}.small{font-size:12px}
.header{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:28px;align-items:end;margin-bottom:24px}
.hero{text-align:right}
.hero .label{font-size:12px;color:var(--ink-2)}
.hero .figure{font-size:56px;line-height:1.05;font-weight:650;letter-spacing:-.03em}
.hero .sub{font-size:13px;color:var(--ink-2);max-width:360px;margin-left:auto}
.tiles{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px;margin:0 0 16px}
.tile{background:var(--surface);border:1px solid var(--line);border-radius:12px;padding:16px 18px;box-shadow:var(--shadow);min-width:0}
.tile .label{font-size:12px;color:var(--ink-2)}
.tile .value{font-size:26px;font-weight:640;letter-spacing:-.02em;line-height:1.25;margin:6px 0 2px}
.tile .sub{font-size:12px;color:var(--ink-2);overflow-wrap:anywhere}
.panel{background:var(--surface);border:1px solid var(--line);border-radius:14px;padding:22px 24px;margin-top:16px;box-shadow:var(--shadow);min-width:0}
.panel-head{display:flex;justify-content:space-between;align-items:flex-start;gap:20px;margin-bottom:16px;flex-wrap:wrap}
.panel-head .sub{color:var(--ink-2);font-size:13px;margin-top:4px;max-width:760px}
.kicker{font-size:11px;font-weight:650;letter-spacing:.08em;text-transform:uppercase;color:var(--accent);margin-bottom:4px}
.nav{position:sticky;top:0;z-index:20;background:color-mix(in srgb,var(--page) 88%,transparent);backdrop-filter:blur(8px);border-bottom:1px solid var(--line);margin:0 -32px 8px;padding:0 32px}
.nav-inner{display:flex;gap:22px;overflow-x:auto;white-space:nowrap;scrollbar-width:none}
.nav a{display:block;padding:14px 0;font-size:13px;font-weight:560;color:var(--ink-2);text-decoration:none;border-bottom:2px solid transparent}
.nav a:hover{color:var(--ink)}.nav a.active{color:var(--ink);border-bottom-color:var(--accent)}
.controls{display:flex;flex-wrap:wrap;gap:10px 12px;align-items:flex-end;margin-bottom:12px}
.field{display:flex;flex-direction:column;gap:5px;font-size:12px;color:var(--ink-2);min-width:0}
.field input,.field select{background:var(--surface);border:1px solid var(--axis);border-radius:8px;padding:8px 10px;color:var(--ink);min-width:0}
.field input[type=search]{width:280px;max-width:100%}
.grow{flex:1}
.btn{background:var(--surface);border:1px solid var(--axis);border-radius:8px;padding:8px 12px;cursor:pointer;white-space:nowrap;color:var(--ink)}
.btn:hover{background:var(--surface-2)}.btn.primary{background:var(--accent);border-color:var(--accent);color:#fff}
.btn.link{border-color:transparent;background:transparent;color:var(--accent);padding:2px 0}
.btn:disabled{opacity:.5;cursor:default}
.seg{display:inline-flex;border:1px solid var(--axis);border-radius:8px;overflow:hidden}
.seg button{border:0;background:var(--surface);padding:8px 12px;cursor:pointer;border-right:1px solid var(--axis)}
.seg button:last-child{border-right:0}.seg button[aria-pressed=true]{background:var(--accent-wash);color:var(--ink);font-weight:600}
.statusline{display:flex;justify-content:space-between;gap:12px;flex-wrap:wrap;font-size:12px;color:var(--ink-2);margin:4px 0 8px}
.table-wrap{overflow:auto;border:1px solid var(--line);border-radius:10px;background:var(--surface)}
.table-wrap.tall{max-height:72vh}
table.data{border-collapse:separate;border-spacing:0;width:100%;font-size:13px}
table.data th{position:sticky;top:0;z-index:1;background:var(--surface-2);text-align:left;font-size:12px;font-weight:600;color:var(--ink-2);padding:9px 12px;white-space:nowrap;border-bottom:1px solid var(--line)}
table.data td{padding:9px 12px;border-bottom:1px solid var(--line);vertical-align:middle}
table.data tr:last-child td{border-bottom:0}
table.data tbody tr:hover td{background:var(--accent-wash)}
table.data .num{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
table.data th.num{text-align:right}
table.data th button{all:unset;cursor:pointer}
table.data th button:focus-visible{outline:2px solid var(--accent)}
.name{font-weight:600}
.empty{text-align:center;color:var(--ink-2);padding:28px}
.score{font-size:15px;font-weight:620;font-variant-numeric:tabular-nums}
.p{display:block;font-size:11px;color:var(--muted);font-variant-numeric:tabular-nums}
.badge{display:inline-flex;align-items:center;gap:4px;border-radius:6px;padding:1px 7px;font-size:11.5px;font-weight:600;white-space:nowrap;border:1px solid transparent}
.badge.pass{color:var(--good-ink);background:color-mix(in srgb,var(--good) 12%,transparent)}
.badge.fail{color:var(--ink-2);background:var(--surface-2)}
.badge.warn{color:var(--warn-ink);background:color-mix(in srgb,var(--warn) 16%,transparent)}
.badge.bad{color:var(--bad-ink);background:color-mix(in srgb,var(--bad) 12%,transparent)}
.meter{display:inline-block;vertical-align:middle;width:64px;height:6px;border-radius:3px;background:var(--surface-2);overflow:hidden;margin-right:8px}
.meter i{display:block;height:100%;border-radius:3px;background:var(--accent)}
.note{border-left:3px solid var(--warn);background:color-mix(in srgb,var(--warn) 8%,var(--surface));padding:10px 14px;border-radius:0 8px 8px 0;font-size:13px;color:var(--ink)}
.cols{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:18px}
.cols p{color:var(--ink-2);font-size:13px}
details.method{margin-top:12px;font-size:13px;color:var(--ink-2)}
details.method summary{cursor:pointer;color:var(--ink);font-weight:560}
details.method p{margin:8px 0}
tbody.unsupported summary{cursor:pointer}
tbody.unsupported .key{margin-top:6px}
pre.evidence{white-space:pre-wrap;overflow-wrap:anywhere;font:12px/1.5 ui-monospace,"Cascadia Mono",Consolas,monospace;background:var(--surface-2);border-radius:8px;padding:12px;margin:8px 0;max-height:360px;overflow:auto}
.kv{display:grid;grid-template-columns:max-content minmax(0,1fr);gap:4px 16px;font-size:13px}
.kv dt{color:var(--ink-2)}.kv dd{margin:0;font-variant-numeric:tabular-nums;overflow-wrap:anywhere}
.footer{display:flex;justify-content:space-between;gap:16px;flex-wrap:wrap;margin-top:28px;font-size:12px;color:var(--muted)}
dialog.sheet{width:min(1180px,calc(100vw - 32px));max-height:calc(100vh - 48px);border:1px solid var(--line);border-radius:14px;background:var(--surface);color:var(--ink);padding:0;box-shadow:0 24px 70px rgba(0,0,0,.35)}
dialog.sheet::backdrop{background:rgba(10,12,16,.55)}
.sheet-head{position:sticky;top:0;z-index:2;display:flex;justify-content:space-between;align-items:flex-start;gap:16px;padding:18px 22px;border-bottom:1px solid var(--line);background:var(--surface)}
.sheet-sub{font-size:12px;color:var(--ink-2);margin-top:4px}
.sheet-body{padding:18px 22px 24px}
.sheet-body h3{margin-top:18px}
.chart{position:relative;width:100%;min-width:0}
.chart svg{display:block;width:100%;overflow:visible}
.chart .grid line{stroke:var(--line);stroke-width:1;shape-rendering:crispEdges}
.chart .axis line{stroke:var(--axis);stroke-width:1;shape-rendering:crispEdges}
.chart text{fill:var(--ink-2);font-size:11px;font-variant-numeric:tabular-nums}
.chart .series path{fill:none;stroke-width:2;stroke-linejoin:round;stroke-linecap:round}
.chart .cross{stroke:var(--ink-2);stroke-width:1;shape-rendering:crispEdges}
.chart .hit{fill:transparent;cursor:crosshair}
.chart .bar{transition:opacity .12s}
.legend{display:flex;flex-wrap:wrap;gap:6px 16px;font-size:12px;color:var(--ink-2);margin:10px 0 0}
.legend .item{display:inline-flex;align-items:center;gap:7px}
.legend .key-line{display:inline-block;width:16px;height:2px;border-radius:1px}
.legend .key-dot{display:inline-block;width:9px;height:9px;border-radius:50%}
.tooltip{position:fixed;z-index:50;pointer-events:none;background:var(--surface);color:var(--ink);border:1px solid var(--line);border-radius:10px;box-shadow:0 10px 30px rgba(0,0,0,.22);padding:9px 11px;font-size:12px;min-width:170px;max-width:340px}
.tooltip .t-head{color:var(--ink-2);margin-bottom:6px;font-variant-numeric:tabular-nums}
.tooltip .t-row{display:grid;grid-template-columns:14px auto minmax(0,1fr);align-items:center;gap:8px;line-height:1.7}
.tooltip .t-row b{font-variant-numeric:tabular-nums;font-weight:650}
.tooltip .t-row span{color:var(--ink-2);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.tooltip .t-key{width:14px;height:2px;border-radius:1px}
@media (max-width:900px){.tiles{grid-template-columns:repeat(2,minmax(0,1fr))}.cols{grid-template-columns:1fr}.header{grid-template-columns:1fr}.hero{text-align:left}.hero .sub{margin-left:0}}
@media (max-width:600px){.page{padding:20px 16px 40px}.nav{margin:0 -16px 8px;padding:0 16px}.panel{padding:16px}.tiles{gap:8px}.tile{padding:12px 14px}.tile .value{font-size:21px}.hero .figure{font-size:44px}.field{flex:1 1 140px}.field input[type=search]{width:100%}.controls>.field:first-child{flex-basis:100%}table.data{font-size:12px}table.data td,table.data th{padding:8px}}
@media print{.nav,.controls,.btn{display:none}.panel,.tile{box-shadow:none}}
'''


BASE_JS = r'''
const Q=(()=>{
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const fin=x=>typeof x==='number'&&Number.isFinite(x);
const num=(x,d=2)=>fin(x)?x.toLocaleString('en-US',{minimumFractionDigits:d,maximumFractionDigits:d}):'—';
const int=x=>fin(x)?Math.round(x).toLocaleString('en-US'):'—';
const pct=(x,d=2)=>fin(x)?num(x,d)+'%':'—';
const signed=(x,d=2)=>fin(x)?(x>0?'+':x<0?'−':'')+num(Math.abs(x),d)+'%':'—';
const mult=(x,d=2)=>fin(x)?num(x,d)+'×':'—';
const money=(x,d=2)=>fin(x)?num(x,d)+'€':'—';
const plural=(n,word,many)=>int(n)+' '+(n===1?word:(many||word+'s'));
// Candidates that cannot vote, one row per reason (often dozens share one): count, reason, folded keys.
const unsupported=(map,span)=>{const groups=new Map();
for(const[k,v]of Object.entries(map||{})){const r=String(v??'').replace(/^(?:error|unsupported):\s*/,'').replace(/^\w*(?:Error|Exception):\s*/,'')||'No reason given';groups.set(r,[...(groups.get(r)||[]),k])}
return groups.size?'<tbody class="unsupported">'+[...groups].sort((a,b)=>b[1].length-a[1].length).map(([r,keys])=>'<tr><td colspan="'+span+'"><details><summary><b>Not evaluable · '+plural(keys.length,'indicator')+'</b> <span class="muted">'+esc(r)+'</span></summary><div class="key">'+keys.map(esc).join(', ')+'</div></details></td></tr>').join('')+'</tbody>':''};
const compact=x=>{if(!fin(x))return'—';const a=Math.abs(x);for(const[v,s]of[[1e12,'T'],[1e9,'B'],[1e6,'M'],[1e3,'K']])if(a>=v)return(x/v).toLocaleString('en-US',{maximumFractionDigits:a/v<10?1:0})+s;return x.toLocaleString('en-US',{maximumFractionDigits:a<10?2:0})};
const $=id=>document.getElementById(id);
const css=name=>getComputedStyle(document.documentElement).getPropertyValue(name).trim();
function csv(columns,rows){const q=v=>v==null?'':typeof v==='string'?'"'+v.replaceAll('"','""')+'"':typeof v==='object'?'"'+JSON.stringify(v).replaceAll('"','""')+'"':String(v);return'﻿'+[columns.map(q).join(','),...rows.map(r=>columns.map(c=>q(r[c])).join(','))].join('\r\n')+'\r\n'}
function download(name,text,type='text/csv;charset=utf-8'){const url=URL.createObjectURL(new Blob([text],{type}));const a=document.createElement('a');a.href=url;a.download=name;document.body.append(a);a.click();a.remove();setTimeout(()=>URL.revokeObjectURL(url),3000)}
function sheet(kicker,title,sub,body){$('sheet-kicker').textContent=kicker||'';$('sheet-title').textContent=title||'';$('sheet-sub').textContent=sub||'';$('sheet-body').innerHTML=body;const d=$('sheet');if(!d.open)d.showModal();$('sheet-body').scrollTop=0}
$('sheet-close').onclick=()=>$('sheet').close();
$('sheet').addEventListener('click',e=>{if(e.target===$('sheet'))$('sheet').close()});
const tip=$('tooltip');
function showTip(html,x,y){tip.innerHTML=html;tip.hidden=false;const w=tip.offsetWidth,h=tip.offsetHeight;let left=x+16,top=y+16;if(left+w>innerWidth-8)left=x-w-16;if(top+h>innerHeight-8)top=Math.max(8,y-h-16);tip.style.left=Math.max(8,left)+'px';tip.style.top=top+'px'}
function hideTip(){tip.hidden=true}
function tipRows(head,rows){return'<div class="t-head">'+esc(head)+'</div>'+rows.map(r=>'<div class="t-row"><i class="t-key" style="background:'+r.color+'"></i><b>'+esc(r.value)+'</b><span>'+esc(r.label)+'</span></div>').join('')}
function svg(tag,attrs={},parent){const e=document.createElementNS('http://www.w3.org/2000/svg',tag);for(const[k,v]of Object.entries(attrs))e.setAttribute(k,v);if(parent)parent.append(e);return e}
function niceStep(span,count){const raw=span/Math.max(1,count),p=10**Math.floor(Math.log10(raw)),f=raw/p;return(f<1.5?1:f<3?2:f<7?5:10)*p}
function linearTicks(lo,hi,count=5){if(!(hi>lo)){hi=lo+1}const s=niceStep(hi-lo,count),out=[];for(let v=Math.ceil(lo/s)*s;v<=hi+s*1e-9;v+=s)out.push(+v.toPrecision(12));return out}
function logTicks(lo,hi){const out=[],a=Math.floor(Math.log10(lo)),b=Math.ceil(Math.log10(hi));const steps=b-a<=2?[1,2,5]:[1];for(let e=a;e<=b;e++)for(const m of steps){const v=m*10**e;if(v>=lo*0.999&&v<=hi*1.001)out.push(v)}return out.length>=2?out:linearTicks(lo,hi,4)}
/* Min/max per pixel column keeps drawdown spikes when 2,000+ days share 900px. */
function thin(points,width){if(points.length<=width*2)return points;const out=[],per=points.length/width;for(let c=0;c<width;c++){const a=Math.floor(c*per),b=Math.min(points.length,Math.floor((c+1)*per));let lo=a,hi=a;for(let i=a;i<b;i++){if(points[i][1]<points[lo][1])lo=i;if(points[i][1]>points[hi][1])hi=i}if(lo<=hi){out.push(points[lo]);if(hi!==lo)out.push(points[hi])}else{out.push(points[hi]);out.push(points[lo])}}return out}
/* Line chart: one y-axis, hairline grid, 2px lines, crosshair + one tooltip for every series. */
function lineChart(host,o){
 host.innerHTML='';host.classList.add('chart');
 const W=Math.max(300,host.clientWidth||900),H=o.height||340,m={l:o.left||62,r:18,t:12,b:30},iw=W-m.l-m.r,ih=H-m.t-m.b;
 const root=svg('svg',{viewBox:`0 0 ${W} ${H}`,height:H,role:'img','aria-label':o.label||'Chart'},host);
 const n=o.dates.length;if(!n||!o.series.length){svg('text',{x:m.l,y:m.t+20},root).textContent='No data';return}
 const ys=o.series.flatMap(s=>s.values.filter(fin));let lo=Math.min(...ys),hi=Math.max(...ys);
 if(o.scale==='log'){lo=Math.max(lo,1e-9);hi=Math.max(hi,lo*1.0001)}else{if(o.zero!==false){lo=Math.min(lo,0);hi=Math.max(hi,0)}if(hi===lo)hi=lo+1}
 const fy=o.scale==='log'?(v=>m.t+ih-(Math.log(v)-Math.log(lo))/(Math.log(hi)-Math.log(lo)||1)*ih):(v=>m.t+ih-(v-lo)/(hi-lo||1)*ih);
 const fx=i=>m.l+(n<2?iw/2:i/(n-1)*iw);
 const grid=svg('g',{class:'grid'},root),axis=svg('g',{class:'axis'},root);
 const ticks=o.scale==='log'?logTicks(lo,hi):linearTicks(lo,hi,5);
 for(const t of ticks){const y=fy(t);if(y<m.t-1||y>m.t+ih+1)continue;svg('line',{x1:m.l,x2:m.l+iw,y1:y,y2:y},grid);const tx=svg('text',{x:m.l-8,y:y+4,'text-anchor':'end'},root);tx.textContent=(o.tick||compact)(t)}
 svg('line',{x1:m.l,x2:m.l+iw,y1:m.t+ih,y2:m.t+ih},axis);
 let last='';const every=Math.max(1,Math.ceil(n/Math.max(2,Math.floor(iw/70))));
 const years=[];for(let i=0;i<n;i++){const y=o.dates[i].slice(0,4);if(y!==last){years.push(i);last=y}}
 const stride=Math.max(1,Math.ceil(years.length/Math.max(2,Math.floor(iw/64))));
 years.forEach((i,k)=>{if(k%stride)return;const x=fx(i);svg('line',{x1:x,x2:x,y1:m.t+ih,y2:m.t+ih+4},axis);const t=svg('text',{x:x,y:H-8,'text-anchor':k===0&&i===0?'start':'middle'},root);t.textContent=o.dates[i].slice(0,4)});
 const lines=svg('g',{class:'series'},root);
 for(const s of o.series){const pts=[];s.values.forEach((v,i)=>{if(fin(v)&&(o.scale!=='log'||v>0))pts.push([i,v])});const t=thin(pts,iw);const d=t.map((p,k)=>(k?'L':'M')+fx(p[0]).toFixed(1)+' '+fy(p[1]).toFixed(1)).join('');const path=svg('path',{d,stroke:s.color},lines);if(s.dash)path.setAttribute('stroke-dasharray','5 4')}
 if(o.series.length<=4){const ends=o.series.map(s=>{let i=s.values.length-1;while(i>=0&&!fin(s.values[i]))i--;return{ s,i,y:i>=0?fy(s.values[i]):NaN}}).filter(e=>e.i>=0);ends.sort((a,b)=>a.y-b.y);const clear=ends.every((e,k)=>!k||e.y-ends[k-1].y>=14);for(const e of ends){svg('circle',{cx:fx(e.i),cy:e.y,r:4,fill:e.s.color,stroke:css('--surface'),'stroke-width':2},root)}if(clear&&o.endLabel){for(const e of ends){const t=svg('text',{x:fx(e.i)-8,y:e.y-8,'text-anchor':'end'},root);t.textContent=o.endLabel(e.s.values[e.i],e.s)}}}
 const cross=svg('line',{class:'cross',y1:m.t,y2:m.t+ih,visibility:'hidden'},root);
 const dots=o.series.map(s=>svg('circle',{r:4,fill:s.color,stroke:css('--surface'),'stroke-width':2,visibility:'hidden'},root));
 const hit=svg('rect',{class:'hit',x:m.l,y:m.t,width:iw,height:ih,tabindex:0,'aria-label':(o.label||'Chart')+': use the arrow keys to read values'},root);
 let cur=n-1;
 function at(i,cx,cy){cur=Math.max(0,Math.min(n-1,i));const x=fx(cur);cross.setAttribute('x1',x);cross.setAttribute('x2',x);cross.setAttribute('visibility','visible');
  const rows=o.series.map((s,k)=>{const v=s.values[cur];if(fin(v)&&(o.scale!=='log'||v>0)){dots[k].setAttribute('cx',x);dots[k].setAttribute('cy',fy(v));dots[k].setAttribute('visibility','visible')}else dots[k].setAttribute('visibility','hidden');return{color:s.color,label:s.label,v,value:fin(v)?(o.format||num)(v):'—'}}).sort((a,b)=>(fin(b.v)?b.v:-Infinity)-(fin(a.v)?a.v:-Infinity));
  const r=host.getBoundingClientRect();showTip(tipRows(o.dates[cur],rows),cx??(r.left+x*(r.width/W)),cy??(r.top+m.t+20));if(o.onMove)o.onMove(cur)}
 function off(){cross.setAttribute('visibility','hidden');dots.forEach(d=>d.setAttribute('visibility','hidden'));hideTip()}
 hit.addEventListener('pointermove',e=>{const r=root.getBoundingClientRect(),x=(e.clientX-r.left)*(W/r.width);at(Math.round((x-m.l)/iw*(n-1)),e.clientX,e.clientY)});
 hit.addEventListener('pointerleave',off);hit.addEventListener('blur',off);
 hit.addEventListener('focus',()=>at(cur));
 hit.addEventListener('keydown',e=>{const step=e.shiftKey?30:1;if(e.key==='ArrowLeft'){at(cur-step);e.preventDefault()}else if(e.key==='ArrowRight'){at(cur+step);e.preventDefault()}else if(e.key==='Home'){at(0);e.preventDefault()}else if(e.key==='End'){at(n-1);e.preventDefault()}else if(e.key==='Escape')off()});
 return{at}
}
/* Column chart for counts: <=24px columns, 4px rounded ends, single series, hover per column. */
function columns(host,o){
 host.innerHTML='';host.classList.add('chart');
 const W=Math.max(300,host.clientWidth||900),H=o.height||220,m={l:44,r:12,t:o.marker!=null?26:12,b:30},iw=W-m.l-m.r,ih=H-m.t-m.b;
 const root=svg('svg',{viewBox:`0 0 ${W} ${H}`,height:H,role:'img','aria-label':o.label||'Chart'},host);
 // Counts: whole-number ticks only, and the top tick at or above the tallest column.
 const hi=Math.max(1,...o.values),step=Math.max(1,niceStep(hi,4)),top=Math.ceil(hi/step)*step,ticks=[];for(let t=0;t<=top;t+=step)ticks.push(t);
 const fy=v=>m.t+ih-v/top*ih,slot=iw/o.values.length,bw=Math.min(24,slot-4);
 const grid=svg('g',{class:'grid'},root);for(const t of ticks){svg('line',{x1:m.l,x2:m.l+iw,y1:fy(t),y2:fy(t)},grid);svg('text',{x:m.l-8,y:fy(t)+4,'text-anchor':'end'},root).textContent=int(t)}
 o.values.forEach((v,i)=>{const x=m.l+slot*i+(slot-bw)/2,y=fy(v),h=m.t+ih-y;const g=svg('g',{},root);if(v>0){const r=Math.min(4,h);svg('path',{class:'bar',d:`M${x} ${m.t+ih}V${y+r}Q${x} ${y} ${x+r} ${y}H${x+bw-r}Q${x+bw} ${y} ${x+bw} ${y+r}V${m.t+ih}Z`,fill:o.colors?.[i]||css('--s1')},g)}
  const hitbox=svg('rect',{x:m.l+slot*i,y:m.t,width:slot,height:ih,fill:'transparent',tabindex:0,'aria-label':o.tip(i)},g);
  const show=e=>{g.querySelector('.bar')?.setAttribute('opacity','.75');const r=hitbox.getBoundingClientRect();showTip('<div class="t-head">'+esc(o.labels[i])+'</div><div class="t-row"><i class="t-key" style="background:'+(o.colors?.[i]||css('--s1'))+'"></i><b>'+esc(int(v))+'</b><span>'+esc(typeof o.unit==='function'?o.unit(v):(o.unit||''))+'</span></div>',e?.clientX??r.left+r.width/2,e?.clientY??r.top)};
  const hide=()=>{g.querySelector('.bar')?.removeAttribute('opacity');hideTip()};
  hitbox.addEventListener('pointermove',show);hitbox.addEventListener('pointerleave',hide);hitbox.addEventListener('focus',()=>show());hitbox.addEventListener('blur',hide)});
 svg('line',{x1:m.l,x2:m.l+iw,y1:m.t+ih,y2:m.t+ih,stroke:css('--axis')},root);
 o.labels.forEach((l,i)=>{if(i%(o.labelEvery||1))return;svg('text',{x:m.l+slot*i+slot/2,y:H-10,'text-anchor':'middle'},root).textContent=o.short?o.short(i):l});
 // The threshold's label sits above the plot, never on a column.
 if(o.marker!=null){const x=m.l+slot*o.marker,right=x>m.l+iw*0.6;svg('line',{x1:x,x2:x,y1:m.t-8,y2:m.t+ih,stroke:css('--ink-2'),'stroke-width':1},root);svg('text',{x:right?x-6:x+6,y:m.t-12,'text-anchor':right?'end':'start'},root).textContent=o.markerLabel||''}
}
function onResize(fn){let w=0;const ro=new ResizeObserver(es=>{const nw=Math.round(es[0].contentRect.width);if(nw&&Math.abs(nw-w)>2){w=nw;fn()}});return el=>ro.observe(el)}
return{esc,fin,num,int,pct,signed,mult,money,plural,unsupported,compact,$,css,csv,download,sheet,showTip,hideTip,tipRows,svg,lineChart,columns,onResize}
})();
'''


LEGACY_TEMPLATE = r'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>__TITLE__</title>
<style>
:root{color-scheme:dark;font:14px system-ui,sans-serif;background:#0e1520;color:#e4edf6}body{max-width:1400px;margin:auto;padding:32px}h1{font-size:32px;font-weight:550}p{color:#aebed0;line-height:1.7}.eyebrow{color:#72dab4;letter-spacing:.12em;font-size:11px}button,input,select{font:inherit;background:#192535;color:inherit;border:1px solid #34465c;border-radius:6px;padding:10px}button{cursor:pointer}.controls{display:flex;flex-wrap:wrap;gap:10px;margin:24px 0}.table{overflow:auto}table{border-collapse:collapse;width:100%;font-size:12px}th,td{padding:13px 10px;text-align:left;border-bottom:1px solid #2b3a4d}th{color:#aebed0;white-space:nowrap}small{display:block;color:#aebed0}.pass{color:#72dab4}.error{color:#edb38e}dialog{background:#101b2a;color:inherit;border:1px solid #435771;border-radius:12px;width:min(1000px,90vw);max-height:85vh}dialog::backdrop{background:#000b}pre{white-space:pre-wrap;overflow-wrap:anywhere;font-size:11px}svg{width:100%;max-height:320px}#chart{border:1px solid #2b3a4d;padding:16px;border-radius:8px;margin:22px 0}.cards{display:flex;gap:30px;flex-wrap:wrap;padding:20px 0}.cards strong{font-size:25px;display:block}a{color:#86b6ff}@media(max-width:600px){body{padding:18px}h1{font-size:25px}}
</style></head><body><div class="eyebrow">QUANTSCRIPT / RESEARCH</div><h1>__TITLE__</h1><p id="description"></p><div class="cards" id="cards"></div>
<div class="controls"><input id="search" placeholder="Find an indicator or candidate" aria-label="Search results"><select id="filter" aria-label="Filter results"><option value="all">All results</option><option value="passed">Overall Gauntlet passes</option><option value="failed">Errors / uncertified</option><option value="single">Single indicators</option><option value="aggregate">Aggregates</option></select><select id="sort" aria-label="Sort results"><option value="default">Result order</option><option value="capital">End capital</option><option value="drawdown">Lowest drawdown</option><option value="score">Overall Gauntlet score</option><option value="name">Name</option></select><button id="download">Download JSON evidence</button></div>
<p id="count"></p><div id="chart" hidden><b id="chart-title"></b><p>Historical capital · logarithmic scale</p><svg viewBox="0 0 1000 260" role="img" aria-label="Historical equity curve"><polyline id="curve" fill="none" stroke="#72dab4" stroke-width="2" /></svg><p id="chart-range"></p></div>
<div class="table"><table><thead id="head"></thead><tbody id="rows"></tbody></table></div><details id="coverage" hidden><summary>Aggregate search coverage</summary><div id="coverage-body"></div></details>
<p>Gauntlet scores measure robustness. System profit ranks historical simulations. Neither is a guarantee of future performance. A sampled search does not establish a global optimum. Missing or failed evidence is never treated as a pass.</p>
<dialog id="detail"><button id="close">Close</button><h2 id="detail-title"></h2><pre id="detail-body"></pre></dialog>
<script>
const DATA=__DATA__;
const $=id=>document.getElementById(id),esc=x=>String(x??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])),fmt=(x,d=2)=>typeof x==='number'&&Number.isFinite(x)?x.toLocaleString('en-US',{maximumFractionDigits:d}):'—';
const profit=DATA.kind==='profit',all=DATA.rows||[];
$('description').textContent=profit?'Native system comparison using the same frozen inputs and configured costs. '+(DATA.validation||''):'Full native Gauntlet and anchored walk-forward optimization on 1H, 4H and 1D. Overall certification requires every track.';
$('cards').innerHTML='<div><strong>'+all.length+'</strong>Results</div><div><strong>'+esc(DATA.run.status)+'</strong>Run status</div><div><strong>'+all.filter(r=>r.error).length+'</strong>Recorded errors</div>';
function chart(r){if(!r.equity?.length)return;$('chart').hidden=false;$('chart-title').textContent=r.name;const a=r.equity.map(p=>Math.log(Math.max(p[1],1e-10))),lo=Math.min(...a),hi=Math.max(...a);$('curve').setAttribute('points',a.map((v,i)=>(20+i/(a.length-1)*960)+','+(225-(v-lo)/(hi-lo||1)*200)).join(' '));$('chart-range').textContent=r.equity[0][0]+' to '+r.equity.at(-1)[0];}
function draw(){const q=$('search').value.toLowerCase(),f=$('filter').value,s=$('sort').value;let rows=all.filter(r=>JSON.stringify([r.name,r.key,r.members]).toLowerCase().includes(q)).filter(r=>f==='all'||f==='passed'&&r.certification?.certified||f==='failed'&&(r.error||!profit&&!r.certification?.certified)||f==='single'&&r.size===1||f==='aggregate'&&r.size>=2);if(s!=='default')rows.sort((a,b)=>s==='name'?a.name.localeCompare(b.name):s==='drawdown'?(a.metrics?.max_drawdown_pct??Infinity)-(b.metrics?.max_drawdown_pct??Infinity):s==='capital'?(b.metrics?.net_return_multiplier??-Infinity)-(a.metrics?.net_return_multiplier??-Infinity):(b.certification?.score??-Infinity)-(a.certification?.score??-Infinity));$('count').textContent=rows.length+' of '+all.length+' results';$('head').innerHTML='<tr><th>Candidate</th>'+(profit?'<th>Size</th><th>Capital ×</th><th>Max drawdown %</th><th>Sharpe</th><th>Sortino</th><th>Omega</th>':'<th>Version</th><th>1H</th><th>4H</th><th>1D</th><th>Overall</th>')+'</tr>';$('rows').innerHTML=rows.map(r=>'<tr><td><button data-index="'+all.indexOf(r)+'">'+esc(r.name)+'</button>'+(r.error?'<small class="error">'+esc(r.error)+'</small>':'')+'</td>'+(profit?'<td>'+(r.size||'Baseline')+'</td>'+['net_return_multiplier','max_drawdown_pct','sharpe','sortino','omega'].map(k=>'<td>'+fmt(r.metrics?.[k])+'</td>').join(''):'<td>'+esc(r.role)+'</td>'+['1h','4h','1d'].map(tf=>'<td>'+fmt(r.timeframes?.[tf]?.score,1)+'<small class="'+(r.timeframes?.[tf]?.certified?'pass':'')+'">'+(r.timeframes?.[tf]?.certified?'Pass':'Uncertified')+'</small></td>').join('')+'<td>'+fmt(r.certification?.score,1)+'<small>'+(r.certification?.certified?'Pass':'Uncertified')+'</small></td>')+'</tr>').join('');$('rows').querySelectorAll('button').forEach(b=>b.onclick=()=>{const r=all[+b.dataset.index];chart(r);$('detail-title').textContent=r.name;const {equity,...detail}=r;$('detail-body').textContent=JSON.stringify(detail,null,2);$('detail').showModal()});}
if(DATA.coverage){$('coverage').hidden=false;$('coverage-body').innerHTML='<table><tr><th>Members</th><th>Tested</th><th>Coverage</th></tr>'+DATA.coverage.map(r=>'<tr><td>'+r.size+'</td><td>'+r.tested+'</td><td>'+(r.exhaustive?'Exhaustive':'Sampled')+'</td></tr>').join('')+'</table>';}
for(const id of ['search','filter','sort'])$(id).addEventListener('input',draw);$('close').onclick=()=>$('detail').close();$('download').onclick=()=>{const url=URL.createObjectURL(new Blob([JSON.stringify(DATA,null,2)],{type:'application/json'})),a=document.createElement('a');a.href=url;a.download='research-evidence.json';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000)};draw();const first=all.find(r=>r.equity?.length);if(first)chart(first);
</script></body></html>'''
