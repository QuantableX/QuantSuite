"""The profit overview: the most profitable singles and aggregates in a saved system."""
from .lab_report import page


def render(run, results):
    data = {"run": {k: run.get(k) for k in ("id", "title", "kind", "status", "created_at", "finished_at")}, **results}
    return page(run.get("title") or "Profit overview", BODY, SCRIPT, data)


BODY = r'''
<header class="header">
 <div><div class="eyebrow" id="eyebrow">QuantSuite · System profitability</div><h1 id="title"></h1><p class="lede" id="lede"></p><div class="chips" id="chips"></div></div>
 <div class="hero"><div class="label">Most profitable aggregate</div><div class="figure" id="hero"></div><div class="sub" id="hero-sub"></div></div>
</header>
<nav class="nav" aria-label="Sections"><div class="nav-inner"><a href="#rankings">Rankings</a><a href="#curves">Capital over time</a><a href="#risk">Profit and risk</a><a href="#years">Annual returns</a><a href="#method">Method</a></div></nav>
<div class="tiles" id="tiles"></div>
<section class="panel" id="rankings">
 <div class="panel-head"><div><div class="kicker">01 · Ranked by final capital</div><h2>The candidates that earned the most</h2><div class="sub">Every candidate drives the whole system over the same period, data and costs. Select a name for its members, parameters, holdings and verification.</div></div>
  <div class="controls" style="margin:0"><label class="field">Start capital<input id="capital" type="number" min="1" step="100" value="1000" inputmode="decimal"></label><button class="btn" id="csv" type="button">Download CSV</button></div></div>
 <h3 id="agg-title">Aggregates</h3>
 <div class="table-wrap"><table class="data rank" aria-label="Most profitable aggregates"><thead><tr><th class="num">Rank</th><th>Aggregate</th><th class="num">Members</th><th class="num">Capital ×</th><th class="num capital-head">From 1,000€</th><th class="num">Max drawdown</th><th class="num">CAGR</th><th class="num">Versus current</th><th>Verified</th></tr></thead><tbody id="agg-rows"></tbody></table></div>
 <h3 style="margin-top:22px" id="single-title">Single indicators</h3>
 <div class="table-wrap"><table class="data rank" aria-label="Most profitable single indicators"><thead><tr><th class="num">Rank</th><th>Indicator</th><th class="num">Members</th><th class="num">Capital ×</th><th class="num capital-head">From 1,000€</th><th class="num">Max drawdown</th><th class="num">CAGR</th><th class="num">Versus current</th><th>Verified</th></tr></thead><tbody id="single-rows"></tbody></table></div>
 <p class="small muted" style="margin-top:12px">A capital multiple of 100× turns 1€ into 100€ in the simulation. Maximum drawdown is the largest fall from a previous capital peak. Amounts in € only scale the multiple; there is no currency conversion.</p>
</section>
<section class="panel" id="curves">
 <div class="panel-head"><div><div class="kicker">02 · Capital over time</div><h2>How the capital grew</h2><div class="sub">Daily equity of the selected candidates. Hover or use the arrow keys for the values on a day.</div></div></div>
 <div class="controls">
  <label class="field">Period<select id="period"></select></label>
  <div class="field">Display<div class="seg" role="group" aria-label="Display"><button type="button" data-mode="log" aria-pressed="true">Capital · log</button><button type="button" data-mode="linear" aria-pressed="false">Capital · linear</button><button type="button" data-mode="dd" aria-pressed="false">Drawdown</button></div></div>
  <div class="field">Compare<div class="seg" role="group" aria-label="Presets"><button type="button" data-preset="aggregates">Top aggregates</button><button type="button" data-preset="singles">Top singles</button><button type="button" data-preset="leaders">Leaders</button></div></div>
 </div>
 <div class="pick" id="pick" aria-label="Candidates on the chart"></div>
 <div id="chart"></div><div class="legend" id="legend"></div>
 <p class="small muted" id="chart-note" style="margin-top:8px"></p>
</section>
<section class="panel" id="risk">
 <div class="panel-head"><div><div class="kicker">03 · Profit and risk</div><h2>More capital, and at what drawdown</h2><div class="sub">Higher means more final capital; further left means a smaller worst decline. Select a point for its details.</div></div></div>
 <div class="risk-grid"><div><div id="scatter"></div><div class="legend" id="scatter-legend"></div></div><aside class="insight" id="insight"></aside></div>
</section>
<section class="panel" id="years">
 <div class="panel-head"><div><div class="kicker">04 · Every year is different</div><h2>When the profits were made</h2><div class="sub" id="years-sub"></div></div></div>
 <div class="table-wrap"><table class="data heat" id="heat"></table></div>
 <p class="small muted" style="margin-top:10px">Annual returns are cut from the one continuous run; compounding them gives the full-period multiple. Blue is a gain, red a loss, deeper for larger moves (gains on a logarithmic scale); the number in each cell is the return.</p>
</section>
<section class="panel" id="method">
 <div class="panel-head"><div><div class="kicker">05 · Reading these numbers</div><h2>What was compared, and how far it goes</h2></div></div>
 <div class="coverage" id="coverage"></div>
 <div class="cols" id="method-cols"></div>
 <details class="method"><summary>Search stages</summary><div class="table-wrap" style="margin-top:8px"><table class="data" id="stages"></table></div></details>
 <details class="method"><summary>Terms</summary><p><b>Capital multiple:</b> final capital divided by the starting capital. 2× doubles the capital.</p><p><b>Maximum drawdown:</b> the largest fall from a previous peak. 50% means the capital temporarily halved.</p><p><b>CAGR:</b> the constant yearly growth that gives the same result over the calendar period.</p><p><b>Aggregate:</b> two or more indicators vote on every pair; the majority decides and an even split keeps the previous verdict. Each base indicator casts one vote.</p></details>
</section>
<footer class="footer"><span id="foot-left"></span><span>One offline HTML file · all data embedded</span></footer>
<style>
.rank td:nth-child(2){min-width:240px}.cand{all:unset;cursor:pointer;font-weight:600;color:var(--ink)}.cand:hover{color:var(--accent);text-decoration:underline}.cand:focus-visible{outline:2px solid var(--accent)}
.code{display:inline-block;min-width:30px;font:600 11px ui-monospace,"Cascadia Mono",Consolas,monospace;color:var(--ink-2)}
.up{color:var(--good-ink)}.down{color:var(--bad-ink)}
.pick{display:flex;flex-wrap:wrap;gap:6px;margin:0 0 12px}.pick button{display:inline-flex;align-items:center;gap:6px;border:1px solid var(--axis);background:var(--surface);border-radius:999px;padding:4px 10px;font-size:12px;cursor:pointer}.pick button[aria-pressed=true]{background:var(--accent-wash);border-color:var(--accent);font-weight:600}.pick i{width:12px;height:2px;border-radius:1px;background:var(--line)}
.risk-grid{display:grid;grid-template-columns:minmax(0,1fr) 300px;gap:24px;align-items:start}.insight{border-left:1px solid var(--line);padding-left:20px}.insight .metric{font-size:34px;font-weight:650;letter-spacing:-.02em;margin:4px 0}
.heat td,.heat th{text-align:right;white-space:nowrap}.heat td:first-child,.heat th:first-child{text-align:left}.heat .who{max-width:300px;overflow:hidden;text-overflow:ellipsis}.heat td.cell{font-variant-numeric:tabular-nums;font-weight:600}
.coverage{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px;margin-bottom:18px}.coverage div{border:1px solid var(--line);border-radius:10px;padding:12px 14px;font-size:12px;color:var(--ink-2)}.coverage b{display:block;font-size:22px;color:var(--ink);font-weight:640}
@media(max-width:900px){.risk-grid{grid-template-columns:1fr}.insight{border-left:0;padding-left:0}.coverage{grid-template-columns:repeat(2,minmax(0,1fr))}}
</style>
'''


SCRIPT = r'''
const {esc,num,int,pct,mult,money,$}=Q,run=DATA.run||{},sys=DATA.system||{},D=DATA.dates||[],SUB=DATA.subjects||[],COV=DATA.coverage||{};
const byId=Object.fromEntries(SUB.map(s=>[s.id,s])),base=SUB.find(s=>s.group==='baseline'),aggs=SUB.filter(s=>s.group==='aggregate'),singles=SUB.filter(s=>s.group==='single');
const COLORS=['--s1','--s2','--s3','--s4','--s5','--s6','--s7','--s8'];
const name=s=>(s.names||s.members).join(' + ');
// Long aggregates: the first members, then a count; the full list is in the title and the details.
const short=(s,n=3)=>{const l=s.names||s.members;return l.length<=n?l.join(' + '):l.slice(0,n).join(' + ')+' + '+(l.length-n)+' more'};
const keysShort=(s,n=4)=>s.members.length<=n?s.members.join(' + '):s.members.slice(0,n).join(' + ')+' + '+(s.members.length-n)+' more';
const codeOf=s=>s.code||'';
let capital=1000;
$('title').textContent=run.title||'Profit overview';
$('lede').textContent=`The most profitable single indicators and aggregates in ${sys.name||'the saved system'}, ranked by simulated final capital and compared with its current setup${base?' ('+short(base,4)+')':''}.`;
const chips=[`${sys.start} to ${sys.end}`,sys.description,`Fees ${num(sys.fee_rate*100,2)}% · slippage ${num(sys.slippage_rate*100,2)}% per side`,sys.filter].filter(Boolean);
$('chips').innerHTML=chips.map(c=>'<span class="chip">'+esc(c)+'</span>').join('');
const leader=aggs[0]||singles[0];
$('hero').textContent=leader?mult(leader.metrics.net_return_multiplier):'—';
$('hero-sub').innerHTML=leader?(leader.names||leader.members).slice(0,4).map(esc).join('<br>')+((leader.members.length>4)?'<br>+ '+(leader.members.length-4)+' more':'')+'<br><span class="small">Final capital multiple after costs</span>':'No completed candidate';
const ratio=s=>base&&s?s.metrics.net_return_multiplier/base.metrics.net_return_multiplier:null;
const lowest=[...aggs,...singles].sort((a,b)=>a.metrics.max_drawdown_pct-b.metrics.max_drawdown_pct)[0];
const tiles=[['Best aggregate',aggs[0]?mult(aggs[0].metrics.net_return_multiplier):'—',aggs[0]?(ratio(aggs[0])?num(ratio(aggs[0]),2)+'× the current system':'')+' · '+aggs[0].members.length+' members':'No aggregate'],
 ['Best single indicator',singles[0]?mult(singles[0].metrics.net_return_multiplier):'—',singles[0]?name(singles[0]):'No single'],
 ['Current system',base?mult(base.metrics.net_return_multiplier):'—',base?short(base,3)+' · drawdown '+pct(base.metrics.max_drawdown_pct):'Not available'],
 ['Lowest drawdown in these lists',lowest?pct(lowest.metrics.max_drawdown_pct):'—',lowest?codeOf(lowest)+' · '+lowest.members.length+' member'+(lowest.members.length===1?'':'s')+' · '+mult(lowest.metrics.net_return_multiplier):'']];
$('tiles').innerHTML=tiles.map(t=>'<div class="tile"><div class="label">'+esc(t[0])+'</div><div class="value">'+esc(t[1])+'</div><div class="sub">'+esc(t[2])+'</div></div>').join('');
function versus(s){const r=ratio(s);if(r==null)return'—';const d=(r-1)*100;return'<span class="'+(d>=0?'up':'down')+'">'+(d>=0?'▲ ':'▼ ')+Q.signed(d,1)+'</span>'}
function verified(s){const v=s.verified;if(!v)return'<span class="badge fail">Not checked</span>';return v.passed?'<span class="badge pass" title="Re-run with the native engine: identical holdings on every bar">✓ Identical</span>':'<span class="badge bad">✕ Differs</span>'}
function rows(list){return list.map(s=>'<tr><td class="num"><span class="code">'+esc(codeOf(s))+'</span></td><td><button class="cand" type="button" data-id="'+esc(s.id)+'" title="'+esc(name(s))+'">'+esc(short(s,4))+'</button><div class="key" title="'+esc(s.members.join(' + '))+'">'+esc(keysShort(s))+'</div></td><td class="num">'+s.members.length+'</td><td class="num"><b>'+mult(s.metrics.net_return_multiplier)+'</b></td><td class="num">'+money(capital*s.metrics.net_return_multiplier)+'</td><td class="num">'+pct(s.metrics.max_drawdown_pct)+'</td><td class="num">'+pct(s.metrics.cagr_pct,1)+'</td><td class="num">'+versus(s)+'</td><td>'+verified(s)+'</td></tr>').join('')||'<tr><td colspan="9" class="empty">No candidate in this group.</td></tr>'}
function drawTables(){$('agg-rows').innerHTML=rows(aggs);$('single-rows').innerHTML=rows(singles);document.querySelectorAll('.capital-head').forEach(h=>h.textContent='From '+money(capital,0))}
$('agg-title').textContent=`The ${aggs.length} most profitable aggregates`;$('single-title').textContent=`The ${singles.length} most profitable single indicators`+(COV.dedupe_bases?' (one version per base indicator)':'');
document.addEventListener('click',e=>{const b=e.target.closest('[data-id]');if(b&&!b.closest('.pick'))detail(b.dataset.id)});
$('capital').addEventListener('input',e=>{const v=Number(e.target.value);if(v>0){capital=v;drawTables();drawChart()}});
// ---- curves
const years=[...new Set(D.map(d=>d.slice(0,4)))];
$('period').innerHTML='<option value="0">Whole period</option>'+years.slice(1).map(y=>'<option value="'+D.findIndex(d=>d.startsWith(y))+'">From '+y+'</option>').join('');
let mode='log',from=0;
const all=[...(base?[base]:[]),...aggs,...singles];
// Color follows the candidate, not its position: a fixed slot per chosen candidate.
const slot={};let selected=[];
function setSelected(ids){const keep=selected.filter(id=>ids.includes(id));for(const id of ids)if(!keep.includes(id))keep.push(id);selected=keep.slice(0,8);const used=new Set(selected.map(id=>slot[id]).filter(x=>x!=null));for(const id of Object.keys(slot))if(!selected.includes(id))delete slot[id];for(const id of selected)if(slot[id]==null){let i=0;while([...Object.values(slot)].includes(i))i++;slot[id]=i}drawPick();drawChart()}
const colorOf=id=>byId[id].group==='baseline'?Q.css('--base'):Q.css(COLORS[slot[id]%8]);
function drawPick(){$('pick').innerHTML=all.map(s=>'<button type="button" data-pick="'+esc(s.id)+'" aria-pressed="'+selected.includes(s.id)+'"><i style="background:'+(selected.includes(s.id)?colorOf(s.id):'var(--line)')+'"></i>'+esc(s.group==='baseline'?'Current':codeOf(s))+'</button>').join('');$('chart-note').textContent=selected.length>=8?'Up to eight curves at once; deselect one to add another.':'Select up to eight candidates.'}
$('pick').addEventListener('click',e=>{const b=e.target.closest('[data-pick]');if(!b)return;const id=b.dataset.pick;if(selected.includes(id))setSelected(selected.filter(x=>x!==id));else if(selected.length<8)setSelected([...selected,id])});
document.querySelectorAll('[data-preset]').forEach(b=>b.onclick=()=>{const p=b.dataset.preset;setSelected(p==='aggregates'?[base?.id,...aggs.slice(0,7).map(s=>s.id)].filter(Boolean):p==='singles'?[base?.id,...singles.slice(0,7).map(s=>s.id)].filter(Boolean):[base?.id,aggs[0]?.id,singles[0]?.id].filter(Boolean))});
document.querySelectorAll('[data-mode]').forEach(b=>b.onclick=()=>{mode=b.dataset.mode;document.querySelectorAll('[data-mode]').forEach(x=>x.setAttribute('aria-pressed',String(x===b)));drawChart()});
$('period').addEventListener('change',e=>{from=Number(e.target.value);drawChart()});
function view(s){const eq=s.equity.slice(from),first=eq[0];if(mode==='dd'){let peak=-Infinity;return eq.map(v=>{peak=Math.max(peak,v);return(v/peak-1)*100})}return eq.map(v=>capital*v/first)}
function drawChart(){const series=selected.map(id=>({id,label:(byId[id].group==='baseline'?'Current':codeOf(byId[id]))+' · '+short(byId[id],2),color:colorOf(id),values:view(byId[id])}));
 Q.lineChart($('chart'),{dates:D.slice(from),series,scale:mode==='log'?'log':'linear',zero:mode!=='log'&&mode!=='linear'?true:false,height:380,label:'Capital over time',format:mode==='dd'?(v=>num(v,1)+'%'):(v=>money(v,0)),tick:mode==='dd'?(v=>num(v,0)+'%'):(v=>Q.compact(v)+'€'),endLabel:mode==='dd'?null:(v=>money(v,0))});
 $('legend').innerHTML=series.map(s=>'<span class="item"><i class="key-line" style="background:'+s.color+'"></i>'+esc(s.label)+'</span>').join('')}
// ---- scatter (one series per group: three hues validated together)
function drawScatter(){const host=$('scatter');host.innerHTML='';host.classList.add('chart');const W=Math.max(300,host.clientWidth||700),H=340,m={l:62,r:20,t:14,b:38},iw=W-m.l-m.r,ih=H-m.t-m.b;
 const pts=all.map(s=>({s,x:s.metrics.max_drawdown_pct,y:s.metrics.net_return_multiplier})).filter(p=>Q.fin(p.x)&&p.y>0);if(!pts.length)return;
 const root=Q.svg('svg',{viewBox:`0 0 ${W} ${H}`,height:H,role:'img','aria-label':'Capital multiple against maximum drawdown'},host);
 const xs=pts.map(p=>p.x),ys=pts.map(p=>p.y),x0=Math.max(0,Math.floor(Math.min(...xs)/5)*5-5),x1=Math.ceil(Math.max(...xs)/5)*5+5,y0=Math.min(...ys)/1.4,y1=Math.max(...ys)*1.4;
 const fx=v=>m.l+(v-x0)/(x1-x0)*iw,fy=v=>m.t+ih-(Math.log(v)-Math.log(y0))/(Math.log(y1)-Math.log(y0))*ih;
 const grid=Q.svg('g',{class:'grid'},root);
 for(let v=Math.ceil(x0/10)*10;v<=x1;v+=10){Q.svg('line',{x1:fx(v),x2:fx(v),y1:m.t,y2:m.t+ih},grid);Q.svg('text',{x:fx(v),y:H-14,'text-anchor':'middle'},root).textContent=v+'%'}
 for(const t of[1,2,5,10,20,50,100,200,500,1000,2000,5000,10000,1e5,1e6].filter(t=>t>=y0&&t<=y1)){Q.svg('line',{x1:m.l,x2:m.l+iw,y1:fy(t),y2:fy(t)},grid);Q.svg('text',{x:m.l-8,y:fy(t)+4,'text-anchor':'end'},root).textContent=Q.compact(t)+'×'}
 Q.svg('text',{x:m.l+iw,y:H-2,'text-anchor':'end'},root).textContent='Maximum drawdown →';
 const col={baseline:Q.css('--base'),aggregate:Q.css('--s1'),single:Q.css('--s2')};
 const labelled=new Set([base?.id,...aggs.slice(0,3).map(s=>s.id),...singles.slice(0,3).map(s=>s.id)]);
 for(const p of pts){const g=Q.svg('g',{tabindex:0,role:'button','aria-label':(codeOf(p.s)||'Current')+' '+short(p.s,3)+': '+mult(p.y)+', drawdown '+pct(p.x),'data-id':p.s.id,style:'cursor:pointer'},root);
  Q.svg('circle',{cx:fx(p.x),cy:fy(p.y),r:12,fill:'transparent'},g);Q.svg('circle',{cx:fx(p.x),cy:fy(p.y),r:p.s.group==='baseline'?6:5,fill:col[p.s.group],stroke:Q.css('--surface'),'stroke-width':2},g);
  if(labelled.has(p.s.id))Q.svg('text',{x:fx(p.x)+9,y:fy(p.y)-8},g).textContent=p.s.group==='baseline'?'Current':codeOf(p.s);
  const show=e=>{const r=g.getBoundingClientRect();Q.showTip(Q.tipRows(codeOf(p.s)||'Current',[{color:col[p.s.group],value:mult(p.y),label:short(p.s,3)},{color:'transparent',value:pct(p.x),label:'maximum drawdown'}]),e?.clientX??r.right,e?.clientY??r.top)};
  g.addEventListener('pointermove',show);g.addEventListener('pointerleave',Q.hideTip);g.addEventListener('focus',()=>show());g.addEventListener('blur',Q.hideTip);g.addEventListener('keydown',e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();detail(p.s.id)}})}
 $('scatter-legend').innerHTML=[['Current system',col.baseline],['Aggregates',col.aggregate],['Single indicators',col.single]].map(([l,c])=>'<span class="item"><i class="key-dot" style="background:'+c+'"></i>'+esc(l)+'</span>').join('')}
const eff=[...aggs,...singles].filter(s=>s.metrics.max_drawdown_pct>0).sort((a,b)=>Math.log(b.metrics.net_return_multiplier)/b.metrics.max_drawdown_pct-Math.log(a.metrics.net_return_multiplier)/a.metrics.max_drawdown_pct)[0];
$('insight').innerHTML=eff?'<div class="kicker">Most growth per drawdown</div><div class="metric">'+esc(mult(eff.metrics.net_return_multiplier))+'</div><p class="muted">'+esc(codeOf(eff)+' · '+short(eff,3))+'<br>Maximum drawdown '+esc(pct(eff.metrics.max_drawdown_pct))+'</p><p class="small muted">Ranked by log growth per percent of maximum drawdown. A historical comparison, not a promise of future stability.</p>':'';
// ---- annual returns heatmap (diverging blue/red, gray midpoint, value in every cell)
const yrs=Object.keys((base||all[0]||{}).annual||{});
const lastYear=yrs[yrs.length-1];
$('years-sub').textContent='Return of each calendar year from the continuous equity curve'+(D.length&&!D[D.length-1].endsWith('12-31')?`; ${lastYear} ends on ${D[D.length-1]}.`:'.');
// Gains on a log scale (a +3,000% year must not flatten +70% and +700%), losses linear to -60%.
const shade=v=>v>=0?Math.min(1,Math.log1p(v/100)/Math.log(16)):Math.min(1,-v/60);
function cellColor(v){const pos=Q.css('--pos'),neg=Q.css('--neg'),mid=Q.css('--mid');const t=shade(v);return`color-mix(in srgb, ${v>=0?pos:neg} ${Math.round(t*85)}%, ${mid})`}
function inkFor(v){return shade(v)>.55?'#fff':'var(--ink)'}
$('heat').innerHTML='<thead><tr><th>Candidate</th>'+yrs.map(y=>'<th>'+y+(y===lastYear&&!D[D.length-1].endsWith('12-31')?'*':'')+'</th>').join('')+'<th>Whole period</th></tr></thead><tbody>'+all.map(s=>'<tr><td class="who" title="'+esc(name(s))+'"><span class="code">'+esc(s.group==='baseline'?'Now':codeOf(s))+'</span> '+esc(short(s,2))+'</td>'+yrs.map(y=>{const v=s.annual?.[y];return Q.fin(v)?'<td class="cell" style="background:'+cellColor(v)+';color:'+inkFor(v)+'">'+Q.signed(v,0)+'</td>':'<td>—</td>'}).join('')+'<td class="num"><b>'+mult(s.metrics.net_return_multiplier)+'</b></td></tr>').join('')+'</tbody>';
// ---- method
const cov=[['Candidates',int(COV.keys),`${int(COV.bases)} base indicators`],['Not evaluable',int(Object.keys(COV.unsupported||{}).length),'listed under the search stages'],['Single runs',int(COV.singles),'every candidate on its own'],['Aggregates tested',int(COV.aggregates),`sizes ${COV.sizes?.[0]??2} to ${COV.sizes?.[1]??'—'}`]];
$('coverage').innerHTML=cov.map(c=>'<div><b>'+esc(c[1])+'</b>'+esc(c[0])+'<br>'+esc(c[2])+'</div>').join('');
const M=[['Same system logic',sys.logic||'Every candidate replaces only the trend signal of the saved system; universe, ranking, filter and execution stay unchanged.'],
 ['Same data and costs',`${sys.start} to ${sys.end}, ${int(D.length)} bars. Fees ${num(sys.fee_rate*100,2)}% and slippage ${num(sys.slippage_rate*100,2)}% per fill. Prices and rankings were frozen once and shared by every candidate.`],
 ['Ranked by profit',`Final capital over the whole period, retrospectively. These parameters and this history were already examined, so this is not an untouched out-of-sample test.`],
 ['Coverage and limits',COV.note||'A finite search: small sizes exhaustively where they fit the budget, larger ones by seeded samples and local search. The best possible aggregate may lie outside the tested set.'],
 ['Existing parameters','Each candidate runs with its library version\'s parameters and the system\'s parameter overrides. Nothing was tuned for this report.'],
 ['Verified',`Every candidate shown was re-run with the unchanged native engine: ${COV.verified_passed??'—'} of ${COV.verified??'—'} with identical holdings on every bar and the same capital.`]];
$('method-cols').innerHTML=M.map(m=>'<div><h3>'+esc(m[0])+'</h3><p>'+esc(m[1])+'</p></div>').join('');
$('stages').innerHTML='<thead><tr><th>Stage</th><th class="num">Proposed</th><th class="num">Tested</th></tr></thead><tbody>'+(COV.stages||[]).map(s=>'<tr><td>'+esc(s.stage)+'</td><td class="num">'+int(s.proposed)+'</td><td class="num">'+int(s.tested)+'</td></tr>').join('')+'</tbody>'+(Object.keys(COV.unsupported||{}).length?'<tbody><tr><td colspan="3"><b>Not evaluable:</b> '+Object.entries(COV.unsupported).map(([k,v])=>esc(k)+' ('+esc(v)+')').join('; ')+'</td></tr></tbody>':'');
// ---- details
function detail(id){const s=byId[id];if(!s)return;const m=s.metrics;let h='';
 h+='<dl class="kv" style="margin-bottom:12px"><dt>Final capital</dt><dd><b>'+mult(m.net_return_multiplier)+'</b> · '+money(capital*m.net_return_multiplier)+' from '+money(capital,0)+'</dd><dt>Maximum drawdown</dt><dd>'+pct(m.max_drawdown_pct)+'</dd><dt>CAGR</dt><dd>'+pct(m.cagr_pct,1)+'</dd><dt>Sharpe · Sortino · Omega</dt><dd>'+num(m.sharpe)+' · '+num(m.sortino)+' · '+num(m.omega)+'</dd><dt>Versus current</dt><dd>'+versus(s)+'</dd><dt>Cash share</dt><dd>'+pct(s.cash_pct,1)+' of bars</dd></dl>';
 h+='<h3>Members and parameters</h3><div class="table-wrap"><table class="data"><thead><tr><th>Indicator</th><th>Version</th><th>Key</th><th>Parameters</th><th>Gauntlet</th></tr></thead><tbody>'+s.members.map((k,i)=>{const p=s.params?.[k]||{},c=s.certification?.[k];return'<tr><td>'+esc(s.names?.[i]||k)+'</td><td class="small">'+esc(s.versions?.[i]||'')+'</td><td class="key">'+esc(k)+'</td><td class="mono small">'+(Object.entries(p).map(([a,b])=>esc(a)+' = '+esc(typeof b==='number'?num(b,Number.isInteger(b)?0:4):JSON.stringify(b))).join('<br>')||'—')+'</td><td>'+(c?(c.certified?'<span class="badge pass">✓ Certified</span>':'<span class="badge fail">Uncertified</span>')+' <span class="small muted">'+num(c.score,1)+'</span>':'—')+'</td></tr>'}).join('')+'</tbody></table></div>';
 const counts={};(s.held||[]).forEach(v=>{counts[v]=(counts[v]||0)+1});const tot=(s.held||[]).length||1;const top=Object.entries(counts).sort((a,b)=>b[1]-a[1]).slice(0,8);
 h+='<h3>Holdings</h3><div class="table-wrap"><table class="data"><thead><tr><th>Held</th><th class="num">Share of bars</th></tr></thead><tbody>'+top.map(([v,c])=>'<tr><td>'+esc(DATA.assets?.[v]??'Nothing yet')+'</td><td class="num"><span class="meter"><i style="width:'+(100*c/tot)+'%"></i></span>'+pct(100*c/tot,1)+'</td></tr>').join('')+'</tbody></table></div>';
 h+='<h3>Annual returns</h3><div class="table-wrap"><table class="data"><thead><tr>'+yrs.map(y=>'<th class="num">'+y+'</th>').join('')+'</tr></thead><tbody><tr>'+yrs.map(y=>'<td class="num">'+Q.signed(s.annual?.[y],1)+'</td>').join('')+'</tr></tbody></table></div>';
 const v=s.verified;h+='<h3>Verification</h3><p class="muted">'+(v?(v.passed?'Re-run with the native engine: identical holdings on all '+int(D.length)+' bars, capital difference '+(v.capital_relative_error===0?'none (bit-identical)':Q.num(v.capital_relative_error*100,10)+'%')+'.':'The native re-run differs: '+int(v.holding_mismatches)+' bars with other holdings.'):'This candidate was not re-run natively.')+'</p>';
 Q.sheet(s.group==='baseline'?'Current system':codeOf(s)+' · rank '+s.rank,name(s),s.members.length+' member'+(s.members.length===1?'':'s')+' · '+s.members.join(' + '),h)}
// ---- CSV
$('csv').onclick=()=>{const rows=all.map(s=>({code:s.code||'current',group:s.group,rank:s.rank,members:s.members.join('+'),names:(s.names||[]).join(' + '),capital_multiple:s.metrics.net_return_multiplier,max_drawdown_pct:s.metrics.max_drawdown_pct,cagr_pct:s.metrics.cagr_pct,sharpe:s.metrics.sharpe,sortino:s.metrics.sortino,omega:s.metrics.omega,cash_pct:s.cash_pct,verified:s.verified?.passed??null,params_json:JSON.stringify(s.params||{}),...Object.fromEntries(yrs.map(y=>['return_'+y+'_pct',s.annual?.[y]??null]))}));Q.download('profit-overview.csv',Q.csv(Object.keys(rows[0]||{code:''}),rows))};
// nav highlight
const links=[...document.querySelectorAll('.nav a')];const obs=new IntersectionObserver(es=>{for(const e of es)if(e.isIntersecting)links.forEach(a=>a.classList.toggle('active',a.getAttribute('href')==='#'+e.target.id))},{rootMargin:'-40% 0px -55% 0px'});document.querySelectorAll('section.panel').forEach(s=>obs.observe(s));
$('foot-left').textContent=`${sys.name||'System'} · ${sys.start} to ${sys.end} · generated ${String(run.finished_at||run.created_at||'').slice(0,10)}`;
drawTables();setSelected([base?.id,aggs[0]?.id,singles[0]?.id,aggs[1]?.id].filter(Boolean));drawScatter();
const watch=Q.onResize(()=>{drawChart();drawScatter()});watch($('chart'));
'''
