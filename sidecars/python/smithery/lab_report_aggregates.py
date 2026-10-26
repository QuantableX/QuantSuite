"""The aggregate comparison: a variable-size search against the system's current aggregate."""
from .lab_report import page


def render(run, results):
    data = {"run": {k: run.get(k) for k in ("id", "title", "kind", "status", "created_at", "finished_at")}, **results}
    return page(run.get("title") or "Aggregate comparison", BODY, SCRIPT, data)


BODY = r'''
<header class="header">
 <div><div class="eyebrow">QuantSuite · Aggregate comparison</div><h1 id="headline"></h1><p class="lede" id="lede"></p><div class="chips" id="chips"></div></div>
 <div class="hero"><div class="label">New leader · final capital</div><div class="figure" id="hero"></div><div class="sub" id="hero-sub"></div></div>
</header>
<nav class="nav" aria-label="Sections"><div class="nav-inner"><a href="#leader">Leader and current</a><a href="#top">Top 10</a><a href="#restarts">Later starts</a><a href="#sizes">By size</a><a href="#method">Search and method</a></div></nav>
<div class="tiles" id="tiles"></div>
<section class="panel" id="leader">
 <div class="panel-head"><div><div class="kicker">01 · Same period, same data</div><h2>The new leader against the current aggregate</h2><div class="sub">Capital on a logarithmic scale, and the fall from the previous peak below it. Hover or use the arrow keys for a day's values.</div></div>
  <div class="controls" style="margin:0"><label class="field">Start capital<input id="capital" type="number" min="1" step="100" value="1000"></label></div></div>
 <div id="capital-chart"></div>
 <h3 style="margin-top:18px">Drawdown</h3>
 <div id="dd-chart"></div>
 <div class="legend" id="legend"></div>
</section>
<section class="panel" id="top">
 <div class="panel-head"><div><div class="kicker">02 · Ranked by final capital</div><h2>The ten most profitable aggregates of this search</h2><div class="sub">Each one re-run with the native engine. Select a name for its members, parameters and holdings.</div></div><button class="btn" id="csv" type="button">Download CSV</button></div>
 <div class="table-wrap"><table class="data" id="top-table"><thead><tr><th class="num">Rank</th><th>Aggregate</th><th class="num">Size</th><th class="num">Capital ×</th><th class="num capital-head">From 1,000€</th><th class="num">Max drawdown</th><th class="num">Versus current</th><th>Verified</th></tr></thead><tbody id="top-rows"></tbody></table></div>
</section>
<section class="panel" id="restarts">
 <div class="panel-head"><div><div class="kicker">03 · Later starts</div><h2>Started again at later dates</h2><div class="sub">The engine restarts from each date with fresh capital; these are new runs, not cut-out pieces of the curve. The periods were already part of the search history, so they are diagnostics, not an untouched test.</div></div></div>
 <div class="table-wrap"><table class="data"><thead><tr><th>Start</th><th class="num">New leader · capital</th><th class="num">New leader · max drawdown</th><th class="num">Current · capital</th><th class="num">Current · max drawdown</th></tr></thead><tbody id="restart-rows"></tbody></table></div>
</section>
<section class="panel" id="sizes">
 <div class="panel-head"><div><div class="kicker">04 · Size is part of the search</div><h2>The best tested aggregate of every size</h2><div class="sub" id="sizes-sub"></div></div></div>
 <div id="size-chart"></div>
 <div class="controls" style="margin-top:12px"><label class="field">Find a size or indicator<input id="size-filter" type="search" placeholder="e.g. 7 or tema"></label></div>
 <div class="table-wrap tall"><table class="data"><thead><tr><th class="num">Size</th><th class="num">Tested</th><th class="num">Best capital ×</th><th class="num">Max drawdown</th><th>Members</th><th>Verified</th></tr></thead><tbody id="size-rows"></tbody></table></div>
</section>
<section class="panel" id="method">
 <div class="panel-head"><div><div class="kicker">05 · What was searched</div><h2>Search, verification and limits</h2></div></div>
 <div class="coverage" id="coverage"></div>
 <div class="table-wrap"><table class="data" id="stages"></table></div>
 <div class="cols" id="method-cols" style="margin-top:18px"></div>
</section>
<footer class="footer"><span id="foot-left"></span><span>One offline HTML file · all data embedded</span></footer>
<style>
.cand{all:unset;cursor:pointer;font-weight:600;color:var(--ink)}.cand:hover{color:var(--accent);text-decoration:underline}.cand:focus-visible{outline:2px solid var(--accent)}
.up{color:var(--good-ink)}.down{color:var(--bad-ink)}
.coverage{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px;margin-bottom:18px}.coverage div{border:1px solid var(--line);border-radius:10px;padding:12px 14px;font-size:12px;color:var(--ink-2)}.coverage b{display:block;font-size:22px;color:var(--ink);font-weight:640}
#top-table td:nth-child(2){min-width:280px}
@media(max-width:900px){.coverage{grid-template-columns:repeat(2,minmax(0,1fr))}}
</style>
'''


SCRIPT = r'''
const {esc,num,int,pct,mult,money,$}=Q,run=DATA.run||{},sys=DATA.system||{},D=DATA.dates||[],SUB=DATA.subjects||[],COV=DATA.coverage||{};
const byId=Object.fromEntries(SUB.map(s=>[s.id,s])),ref=SUB.find(s=>s.group==='reference'),ranked=SUB.filter(s=>s.group==='aggregate').sort((a,b)=>a.rank-b.rank),lead=ranked[0];
const names=s=>(s.names||s.members).join(' + ');
// Long aggregates: the first members, then a count; the full list is in the title and the details.
const short=(s,n=3)=>{const l=s.names||s.members;return l.length<=n?l.join(' + '):l.slice(0,n).join(' + ')+' + '+(l.length-n)+' more'};
const keysShort=(keys,n=4)=>keys.length<=n?keys.join(' + '):keys.slice(0,n).join(' + ')+' + '+(keys.length-n)+' more';
let capital=1000;
const gain=lead&&ref?(lead.metrics.net_return_multiplier/ref.metrics.net_return_multiplier-1)*100:null;
const beaten=gain!=null&&gain>0&&lead.members.join()!==ref.members.join();
$('headline').textContent=!lead?'No aggregate completed':beaten?`A ${lead.members.length}-member aggregate earned ${Q.signed(gain,1).replace('+','')} more than the current aggregate`:'The current aggregate remains the most profitable one found';
$('lede').textContent=`A variable-size search over aggregates of two or more distinct indicators in ${sys.name||'the saved system'}, with the same data, universe, filter and costs, ranked by historical final capital. The current aggregate is ${ref?short(ref,4):'—'}.`;
$('chips').innerHTML=[`${sys.start} to ${sys.end}`,sys.description,sys.filter,`${int(COV.aggregates)} aggregates tested`].filter(Boolean).map(c=>'<span class="chip">'+esc(c)+'</span>').join('');
$('hero').textContent=lead?mult(lead.metrics.net_return_multiplier):'—';
$('hero-sub').innerHTML=lead?(lead.names||lead.members).slice(0,4).map(esc).join('<br>')+(lead.members.length>4?'<br>+ '+(lead.members.length-4)+' more':''):'';
const tiles=[['New leader',lead?mult(lead.metrics.net_return_multiplier):'—',lead?`${lead.members.length} indicators · max drawdown ${pct(lead.metrics.max_drawdown_pct)}`:''],
 ['Current aggregate',ref?mult(ref.metrics.net_return_multiplier):'—',ref?`${ref.members.length} indicator${ref.members.length===1?'':'s'} · max drawdown ${pct(ref.metrics.max_drawdown_pct)}`:''],
 ['More final capital',gain==null?'—':Q.signed(gain,1),'leader against the current aggregate'],
 ['Aggregates tested',int(COV.aggregates),`sizes ${COV.sizes?.[0]??2} to ${COV.sizes?.[COV.sizes.length-1]??'—'} · ${int(COV.usable)} usable indicators`]];
$('tiles').innerHTML=tiles.map(t=>'<div class="tile"><div class="label">'+esc(t[0])+'</div><div class="value">'+esc(t[1])+'</div><div class="sub">'+esc(t[2])+'</div></div>').join('');
function curves(){const pair=[lead&&{s:lead,label:'New leader · '+short(lead,3),color:Q.css('--s1')},ref&&{s:ref,label:'Current · '+short(ref,3),color:Q.css('--base')}].filter(Boolean);
 Q.lineChart($('capital-chart'),{dates:D,series:pair.map(p=>({label:p.label,color:p.color,values:p.s.equity.map(v=>capital*v)})),scale:'log',height:320,label:'Capital over time',format:v=>money(v,0),tick:v=>Q.compact(v)+'€',endLabel:v=>money(v,0)});
 Q.lineChart($('dd-chart'),{dates:D,series:pair.map(p=>{let peak=-Infinity;return{label:p.label,color:p.color,values:p.s.equity.map(v=>{peak=Math.max(peak,v);return(v/peak-1)*100})}}),scale:'linear',height:170,label:'Drawdown over time',format:v=>num(v,1)+'%',tick:v=>num(v,0)+'%'});
 $('legend').innerHTML=pair.map(p=>'<span class="item"><i class="key-line" style="background:'+p.color+'"></i>'+esc(p.label)+'</span>').join('')}
function versus(s){if(!ref)return'—';const d=(s.metrics.net_return_multiplier/ref.metrics.net_return_multiplier-1)*100;return'<span class="'+(d>=0?'up':'down')+'">'+(d>=0?'▲ ':'▼ ')+Q.signed(d,1)+'</span>'}
const ok=s=>s.verified?.passed?'<span class="badge pass">✓ Identical</span>':'<span class="badge fail">Not checked</span>';
function drawTop(){$('top-rows').innerHTML=ranked.map(s=>'<tr><td class="num">'+s.rank+'</td><td><button class="cand" type="button" data-id="'+esc(s.id)+'" title="'+esc(names(s))+'">'+esc(short(s,4))+'</button><div class="key" title="'+esc(s.members.join(' + '))+'">'+esc(keysShort(s.members))+'</div></td><td class="num">'+s.members.length+'</td><td class="num"><b>'+mult(s.metrics.net_return_multiplier)+'</b></td><td class="num">'+money(capital*s.metrics.net_return_multiplier)+'</td><td class="num">'+pct(s.metrics.max_drawdown_pct)+'</td><td class="num">'+versus(s)+'</td><td>'+ok(s)+'</td></tr>').join('')||'<tr><td colspan="8" class="empty">No aggregate completed.</td></tr>';document.querySelectorAll('.capital-head').forEach(h=>h.textContent='From '+money(capital,0))}
$('capital').addEventListener('input',e=>{const v=Number(e.target.value);if(v>0){capital=v;drawTop();curves()}});
document.addEventListener('click',e=>{const b=e.target.closest('[data-id]');if(b)detail(b.dataset.id)});
$('restart-rows').innerHTML=(DATA.restarts||[]).map(r=>'<tr><td>'+esc(r.start)+'</td><td class="num"><b>'+mult(r.leader?.net_return_multiplier)+'</b></td><td class="num">'+pct(r.leader?.max_drawdown_pct)+'</td><td class="num">'+mult(r.reference?.net_return_multiplier)+'</td><td class="num">'+pct(r.reference?.max_drawdown_pct)+'</td></tr>').join('')||'<tr><td colspan="5" class="empty">No later start inside the period.</td></tr>';
const BS=COV.best_by_size||[],verifiedSize=Object.fromEntries(SUB.filter(s=>s.group==='size_best'||s.group==='aggregate').map(s=>[s.members.join('+'),s]));
$('sizes-sub').textContent=`${BS.length} sizes tested, from ${BS[0]?.size??'—'} to ${BS[BS.length-1]?.size??'—'} members. Small sizes are tested more thoroughly than large ones; the counts show how much of each size was seen.`;
function drawSizes(){const q=$('size-filter').value.toLowerCase().trim();$('size-rows').innerHTML=BS.filter(r=>!q||String(r.size)===q||r.members.join(' ').toLowerCase().includes(q)).map(r=>{const v=verifiedSize[r.members.join('+')];return'<tr><td class="num">'+r.size+'</td><td class="num">'+int(r.tested)+'</td><td class="num"><b>'+mult(r.net_return_multiplier)+'</b></td><td class="num">'+pct(r.max_drawdown_pct)+'</td><td class="key" title="'+esc(r.members.join(' + '))+'">'+(v?'<button class="cand" type="button" data-id="'+esc(v.id)+'">'+esc(keysShort(r.members,5))+'</button>':esc(keysShort(r.members,5)))+'</td><td>'+(v?ok(v):'<span class="badge fail">Search result</span>')+'</td></tr>'}).join('')||'<tr><td colspan="6" class="empty">No size matches.</td></tr>'}
$('size-filter').addEventListener('input',drawSizes);
function sizeChart(){const host=$('size-chart');host.innerHTML='';host.classList.add('chart');if(!BS.length)return;const W=Math.max(300,host.clientWidth||900),H=230,m={l:62,r:16,t:12,b:34},iw=W-m.l-m.r,ih=H-m.t-m.b;
 const root=Q.svg('svg',{viewBox:`0 0 ${W} ${H}`,height:H,role:'img','aria-label':'Best final capital by aggregate size'},host);
 const xs=BS.map(r=>r.size),ys=BS.map(r=>r.net_return_multiplier).filter(v=>v>0),x0=Math.min(...xs),x1=Math.max(...xs),y0=Math.min(...ys)/1.3,y1=Math.max(...ys)*1.3;
 const logx=x1/x0>20,fx=v=>m.l+(logx?Math.log(v/x0)/Math.log(x1/x0||2):(v-x0)/((x1-x0)||1))*iw,fy=v=>m.t+ih-(Math.log(v)-Math.log(y0))/(Math.log(y1)-Math.log(y0))*ih;
 const grid=Q.svg('g',{class:'grid'},root);
 for(const t of[1,2,5,10,20,50,100,200,500,1000,2000,5000,1e4,1e5].filter(t=>t>=y0&&t<=y1)){Q.svg('line',{x1:m.l,x2:m.l+iw,y1:fy(t),y2:fy(t)},grid);Q.svg('text',{x:m.l-8,y:fy(t)+4,'text-anchor':'end'},root).textContent=Q.compact(t)+'×'}
 for(const t of(logx?[2,3,5,10,20,50,100,200,400]:xs.filter((_,i)=>i%Math.ceil(xs.length/10)===0)).filter(t=>t>=x0&&t<=x1))Q.svg('text',{x:fx(t),y:H-14,'text-anchor':'middle'},root).textContent=t;
 Q.svg('text',{x:m.l+iw,y:H-1,'text-anchor':'end'},root).textContent='Members per aggregate →';
 if(ref)Q.svg('line',{x1:m.l,x2:m.l+iw,y1:fy(ref.metrics.net_return_multiplier),y2:fy(ref.metrics.net_return_multiplier),stroke:Q.css('--base'),'stroke-width':1},root),Q.svg('text',{x:m.l+iw,y:fy(ref.metrics.net_return_multiplier)-6,'text-anchor':'end'},root).textContent='Current '+mult(ref.metrics.net_return_multiplier);
 const d=BS.map((r,i)=>(i?'L':'M')+fx(r.size).toFixed(1)+' '+fy(r.net_return_multiplier).toFixed(1)).join('');Q.svg('path',{d,fill:'none',stroke:Q.css('--s1'),'stroke-width':2,'stroke-linejoin':'round'},root);
 for(const r of BS){const g=Q.svg('g',{tabindex:0,'aria-label':`Size ${r.size}: ${mult(r.net_return_multiplier)}`},root);Q.svg('circle',{cx:fx(r.size),cy:fy(r.net_return_multiplier),r:10,fill:'transparent'},g);Q.svg('circle',{cx:fx(r.size),cy:fy(r.net_return_multiplier),r:3.5,fill:Q.css('--s1'),stroke:Q.css('--surface'),'stroke-width':2},g);
  const show=e=>{const b=g.getBoundingClientRect();Q.showTip(Q.tipRows(r.size+' members · '+int(r.tested)+' tested',[{color:Q.css('--s1'),value:mult(r.net_return_multiplier),label:'best final capital'},{color:'transparent',value:pct(r.max_drawdown_pct),label:'maximum drawdown'}]),e?.clientX??b.right,e?.clientY??b.top)};
  g.addEventListener('pointermove',show);g.addEventListener('pointerleave',Q.hideTip);g.addEventListener('focus',()=>show());g.addEventListener('blur',Q.hideTip)}}
$('coverage').innerHTML=[['Aggregates tested',int(COV.aggregates),`budget ${int(COV.budget)}`],['Usable indicators',int(COV.usable),`${int(COV.bases)} base indicators, ${int(Object.keys(COV.unsupported||{}).length)} not evaluable`],['Re-run natively',int(COV.verified_passed)+' of '+int(COV.verified),'identical holdings on every bar'],['Search time',int(COV.seconds)+' s',COV.evaluator==='bulk'?'bulk evaluation, native checks':'native evaluation']].map(c=>'<div><b>'+esc(c[1])+'</b>'+esc(c[0])+'<br>'+esc(c[2])+'</div>').join('');
$('stages').innerHTML='<thead><tr><th>Stage</th><th class="num">Proposed</th><th class="num">Tested</th></tr></thead><tbody>'+(COV.stages||[]).map(s=>'<tr><td>'+esc(s.stage)+'</td><td class="num">'+int(s.proposed)+'</td><td class="num">'+int(s.tested)+'</td></tr>').join('')+'</tbody>'+Q.unsupported(COV.unsupported,3);
const P=DATA.plan||{};
$('method-cols').innerHTML=[['The rule',P.rule||''],['Fixed before the results',`Stages, budgets and the seed (${P.seed}) were written to the plan before any result was seen; local search starts from the leaders of the stages before it.`],['Limits',`A finite, retrospective search on history that was already examined. Large sizes are sampled, not enumerated; the best aggregate overall may lie outside the tested set. Final capital is not a forecast.`]].map(m=>'<div><h3>'+esc(m[0])+'</h3><p>'+esc(m[1])+'</p></div>').join('');
function detail(id){const s=byId[id];if(!s)return;const m=s.metrics;let h='<dl class="kv" style="margin-bottom:12px"><dt>Final capital</dt><dd><b>'+mult(m.net_return_multiplier)+'</b> · '+money(capital*m.net_return_multiplier)+' from '+money(capital,0)+'</dd><dt>Maximum drawdown</dt><dd>'+pct(m.max_drawdown_pct)+'</dd><dt>CAGR</dt><dd>'+pct(m.cagr_pct,1)+'</dd><dt>Sharpe · Sortino · Omega</dt><dd>'+num(m.sharpe)+' · '+num(m.sortino)+' · '+num(m.omega)+'</dd><dt>Versus current</dt><dd>'+versus(s)+'</dd><dt>Cash share</dt><dd>'+pct(s.cash_pct,1)+' of bars</dd></dl>';
 h+='<h3>Members and parameters</h3><div class="table-wrap"><table class="data"><thead><tr><th>Indicator</th><th>Version</th><th>Key</th><th>Parameters</th><th>Gauntlet</th></tr></thead><tbody>'+s.members.map((k,i)=>{const p=s.params?.[k]||{},c=s.certification?.[k];return'<tr><td>'+esc(s.names?.[i]||k)+'</td><td class="small">'+esc(s.versions?.[i]||'')+'</td><td class="key">'+esc(k)+'</td><td class="mono small">'+(Object.entries(p).map(([a,b])=>esc(a)+' = '+esc(typeof b==='number'?num(b,Number.isInteger(b)?0:4):JSON.stringify(b))).join('<br>')||'—')+'</td><td>'+(c?(c.certified?'<span class="badge pass">✓ Certified</span>':'<span class="badge fail">Uncertified</span>')+' <span class="small muted">'+num(c.score,1)+'</span>':'—')+'</td></tr>'}).join('')+'</tbody></table></div>';
 const counts={};(s.held||[]).forEach(v=>{counts[v]=(counts[v]||0)+1});const tot=(s.held||[]).length||1;
 h+='<h3>Holdings</h3><div class="table-wrap"><table class="data"><thead><tr><th>Held</th><th class="num">Share of bars</th></tr></thead><tbody>'+Object.entries(counts).sort((a,b)=>b[1]-a[1]).slice(0,8).map(([v,c])=>'<tr><td>'+esc(DATA.assets?.[v]??'Nothing yet')+'</td><td class="num"><span class="meter"><i style="width:'+(100*c/tot)+'%"></i></span>'+pct(100*c/tot,1)+'</td></tr>').join('')+'</tbody></table></div>';
 const v=s.verified;h+='<h3>Verification</h3><p class="muted">'+(v?.passed?'Re-run with the native engine: identical holdings on all '+int(D.length)+' bars'+(v.capital_relative_error===0?', bit-identical capital.':'.'):'Not re-run natively.')+'</p>';
 Q.sheet(s.group==='reference'?'Current aggregate':s.group==='aggregate'?'Rank '+s.rank:'Best of size '+s.members.length,names(s),s.members.length+' members · '+s.members.join(' + '),h)}
$('csv').onclick=()=>{const rows=(DATA.rows||[]).map((r,i)=>({rank:i+1,size:r.size,members:r.members.join('+'),capital_multiple:r.metrics.net_return_multiplier,max_drawdown_pct:r.metrics.max_drawdown_pct}));Q.download('aggregate-comparison.csv',Q.csv(Object.keys(rows[0]||{rank:''}),rows))};
const links=[...document.querySelectorAll('.nav a')];const obs=new IntersectionObserver(es=>{for(const e of es)if(e.isIntersecting)links.forEach(a=>a.classList.toggle('active',a.getAttribute('href')==='#'+e.target.id))},{rootMargin:'-40% 0px -55% 0px'});document.querySelectorAll('section.panel').forEach(s=>obs.observe(s));
$('foot-left').textContent=`${sys.name||'System'} · ${sys.start} to ${sys.end} · generated ${String(run.finished_at||run.created_at||'').slice(0,10)}`;
drawTop();drawSizes();curves();sizeChart();const watch=Q.onResize(()=>{curves();sizeChart()});watch($('capital-chart'));
'''
