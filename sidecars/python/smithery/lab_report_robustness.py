"""The System Gauntlet's report: verdict first, then every axis, group and variant."""
from .lab_report import page


def render(run, results):
    data = {"run": {k: run.get(k) for k in ("id", "title", "kind", "status", "created_at", "finished_at")}, **results}
    return page(run.get("title") or "Robustness check", BODY, SCRIPT, data)


BODY = r'''
<style>td.wrap{white-space:normal;min-width:220px;max-width:420px}details.block{margin-top:10px}details.block summary{cursor:pointer;margin-bottom:8px}ul.method{margin:0;padding-left:18px;color:var(--ink-2);font-size:13px}ul.method li{margin-bottom:6px;overflow-wrap:anywhere}</style>
<header class="header">
 <div><div class="eyebrow">QuantSuite · Robustness check (System Gauntlet)</div><h1 id="headline"></h1><p class="lede" id="lede"></p><div class="chips" id="chips"></div></div>
 <div class="hero"><div class="label">Verdict</div><div class="figure" id="hero"></div><div class="sub" id="hero-sub"></div></div>
</header>
<nav class="nav" aria-label="Sections"><div class="nav-inner"><a href="#axes">Axes</a><a href="#groups">Every check</a><a href="#time">Time</a><a href="#mc">Monte Carlo</a><a href="#variants">Every run</a><a href="#method">Method</a></div></nav>
<div class="tiles" id="tiles"></div>
<section class="panel" id="axes">
 <div class="panel-head"><div><div class="kicker">01 · Six axes</div><h2>How the system holds up</h2><div class="sub">Each axis is the mean of its checks' scores. A check scores 0 when a change keeps 40% of the yearly growth (CAGR) and 100 when it keeps 90%. A failed gate makes the system not robust, whatever the score.</div></div></div>
 <div class="table-wrap"><table class="data"><thead><tr><th>Axis</th><th class="num">Score</th><th>Checks</th><th>Failed gates</th></tr></thead><tbody id="axis-rows"></tbody></table></div>
</section>
<section class="panel" id="groups">
 <div class="panel-head"><div><div class="kicker">02 · Every check</div><h2>What each change left of the yearly growth</h2><div class="sub">Kept = CAGR with the change ÷ CAGR of the system as it is. 1.00 keeps everything; below 0.5 most of the edge is gone.</div></div></div>
 <div class="table-wrap"><table class="data"><thead><tr><th>Check</th><th>What changes</th><th class="num">Runs</th><th class="num">Median kept</th><th class="num">Worst kept</th><th class="num">Losing runs</th><th class="num">Score</th><th>Gate</th></tr></thead><tbody id="group-rows"></tbody></table></div>
</section>
<section class="panel" id="time">
 <div class="panel-head"><div><div class="kicker">03 · Time</div><h2>The system as it is, year by year</h2><div class="sub" id="time-sub"></div></div></div>
 <div id="equity-chart"></div>
 <div class="table-wrap"><table class="data"><thead><tr><th>Year</th><th class="num">Return</th></tr></thead><tbody id="year-rows"></tbody></table></div>
</section>
<section class="panel" id="mc">
 <div class="panel-head"><div><div class="kicker">04 · Monte Carlo</div><h2>Luck and path risk</h2><div class="sub">Shuffled calendar days ask whether the system needs the real order of the market (the luck test). The bootstrap redraws the system's own daily returns in 20-day blocks: the spread of growth and drawdown, and the chance of losing over one and three years.</div></div></div>
 <div class="table-wrap"><table class="data"><tbody id="mc-rows"></tbody></table></div>
</section>
<section class="panel" id="variants">
 <div class="panel-head"><div><div class="kicker">05 · Every run</div><h2>Each variant on its own</h2><div class="sub">Member drops, additions and delays run on the engine-identical bulk evaluator against its own base; every other variant is a native engine run.</div></div></div>
 <div id="variant-blocks"></div>
</section>
<section class="panel" id="method">
 <div class="panel-head"><div><div class="kicker">06 · Method</div><h2>Policy, data and limits</h2></div></div>
 <ul class="method" id="method-list"></ul>
</section>
'''

SCRIPT = r'''
const D=DATA;
const {esc,fin,num,int,pct,mult,$,lineChart,onResize}=Q;
const V=D.verdict, G=D.groups, AX=D.axes, META=D.groups_meta||{};
const AXIS={parameters:'Parameters',time:'Time',exchange:'Exchange and data',assets:'Assets and universe',monte_carlo:'Monte Carlo',execution:'Execution'};
const LABEL={parameter_jitter:'Parameter jitter',member_drop:'Member left out',member_add:'Member added',system_settings:'System settings',start_shift:'Later start',eras:'Eras',price_noise:'Price noise',venue:'Other exchange',universe:'Universe',permutation:'Shuffled days',bootstrap:'Return bootstrap',delay:'Trade delay',fees:'Higher fees'};
const kept=x=>fin(x)?num(x,2):'—';
const name=(D.system&&D.system.name)||(D.run&&D.run.title)||'System';
$('headline').textContent=name+': '+(V.status==='robust'?'robust':V.status==='incomplete'?'incomplete':'not robust');
$('lede').textContent=V.status==='robust'?'No gate failed and every axis was measured. Small realistic changes leave most of the yearly growth.'
 :V.status==='incomplete'?'Not every axis could be measured, so this is not a robust verdict. See the method section for what is missing.'
 :'At least one realistic change breaks it. Each failed gate is listed below.';
$('chips').innerHTML=[`Policy ${esc(D.policy)}`,D.fast?'Fast mode':'Full mode',`Seed ${int(D.seed)}`,D.config_window?`${esc(D.config_window.start)} → ${esc(D.config_window.end)}`:''].filter(Boolean).map(c=>`<span class="chip">${c}</span>`).join('');
$('hero').textContent=`${V.grade} · ${fin(V.score)?num(V.score,0):'—'}`;
$('hero-sub').textContent=V.failures.length?`${V.failures.length} failed gate${V.failures.length>1?'s':''}`:(V.perm_p!=null&&V.perm_p>0.1?'Grade capped at B by the luck test':'No failed gate');
const b=D.base;
$('tiles').innerHTML=[['Final multiple',mult(b.multiple)],['CAGR',pct(b.cagr*100,1)],['Max drawdown',pct(b.max_drawdown_pct,1)],['Years',num(b.years,1)]]
 .map(([l,v])=>`<div class="tile"><div class="label">${l}</div><div class="value">${v}</div></div>`).join('');
$('axis-rows').innerHTML=Object.keys(AXIS).map(a=>{const gs=Object.values(G).filter(g=>g.axis===a);const f=gs.filter(g=>g.failed);
 return `<tr><td>${AXIS[a]}</td><td class="num">${fin(AX[a])?num(AX[a],0):'—'}</td><td>${gs.map(g=>esc(LABEL[g.group]||g.group)).join(', ')||'—'}</td><td>${f.length?f.map(g=>esc(LABEL[g.group]||g.group)).join(', '):'—'}</td></tr>`}).join('');
const order=['parameter_jitter','member_drop','member_add','system_settings','start_shift','eras','price_noise','venue','universe','permutation','bootstrap','delay','fees'];
$('group-rows').innerHTML=order.filter(k=>G[k]).map(k=>{const g=G[k],m=META[k]||{};
 const gate=g.failed?`<span class="badge bad">Failed</span> ${esc((g.reasons||[]).join('; '))}`:g.gated?'<span class="badge pass">Passed</span>':`<span class="badge fail">${g.note?esc(g.note):'Not gated'}</span>`;
 const runs=g.n!=null?int(g.n):'—';const flips=fin(g.flips)?pct(g.flips*100,0):'—';
 const also=g.also?Object.entries(g.also).map(([l,v])=>`${esc(l)}: ${kept(v)}`).join(' · '):'';
 return `<tr><td>${esc(LABEL[k]||k)}</td><td class="wrap">${esc(m.what||'')}${also?'<br><span class="muted">'+also+'</span>':''}</td><td class="num">${runs}</td><td class="num">${kept(g.median)}</td><td class="num">${kept(g.min)}</td><td class="num">${flips}</td><td class="num">${fin(g.score)?num(g.score,0):'—'}</td><td class="wrap">${gate}</td></tr>`}).join('');
const E=G.eras||{};
$('time-sub').textContent=`Positive rolling 1-year windows: ${fin(E.rolling_1y_positive)?pct(E.rolling_1y_positive*100,0):'—'} · worst 1-year return: ${fin(E.worst_1y)?pct(E.worst_1y*100,1):'—'} · worst rolling 2-year CAGR: ${fin(E.worst_2y_cagr)?pct(E.worst_2y_cagr*100,1):'—'}.`;
$('year-rows').innerHTML=Object.entries(E.years||{}).map(([y,r])=>`<tr><td>${esc(y)}</td><td class="num">${pct(r*100,1)}</td></tr>`).join('');
const draw=()=>lineChart($('equity-chart'),{dates:D.equity.dates,series:[{name:'Capital ×',color:'var(--s1)',values:D.equity.values}],scale:'log',label:'Capital of the system as it is',tick:v=>Q.compact(v)+'×'});
onResize(draw)($('equity-chart'));draw();
const P=G.permutation||{},B=G.bootstrap||{};
$('mc-rows').innerHTML=[['Luck test p (shuffled days at least as good)',P.p!=null?num(P.p,3)+(P.caps_grade?' · caps the grade at B':''):'—'],
 ['Median CAGR on shuffled days',fin(P.median_shuffled_cagr)?pct(P.median_shuffled_cagr*100,1):'—'],
 ['Bootstrap CAGR, 5th and 50th percentile',fin(B.cagr_p5)?pct(B.cagr_p5*100,1)+' · '+pct(B.cagr_p50*100,1):'—'],
 ['Bootstrap max drawdown, median and 95th percentile',fin(B.dd_p50)?pct(B.dd_p50*100,1)+' · '+pct(B.dd_p95*100,1):'—'],
 ['Chance of a loss over 1 year · 3 years',fin(B.loss_1y)?pct(B.loss_1y*100,1)+' · '+pct(B.loss_3y*100,1):'—']]
 .map(([l,v])=>`<tr><td>${l}</td><td class="num">${v}</td></tr>`).join('');
const det=D.details||{};
$('variant-blocks').innerHTML=Object.keys(det).map(k=>{const rows=det[k]||[];
 return `<details class="block"><summary><b>${esc(LABEL[k]||k)}</b> · ${int(rows.length)} run${rows.length===1?'':'s'}</summary><div class="table-wrap"><table class="data"><thead><tr><th>Run</th><th class="num">Multiple</th><th class="num">CAGR</th><th class="num">Max drawdown</th><th>Note</th></tr></thead><tbody>`+
 rows.map(r=>`<tr><td>${esc(String(r.label||'').split('|').pop())}</td><td class="num">${mult(r.multiple)}</td><td class="num">${fin(r.cagr)?pct(r.cagr*100,1):'—'}</td><td class="num">${fin(r.max_drawdown_pct)?pct(r.max_drawdown_pct,1):'—'}</td><td class="wrap">${esc(r.error||'')}</td></tr>`).join('')+'</tbody></table></div></details>'}).join('');
$('method-list').innerHTML=[
 'The policy is docs/ROBUSTNESS.md in the QuantSuite repository; smithery.robust_policy holds every group, gate and grade.',
 'Every variant runs the unchanged engine on the frozen copy of your saved prices, changed in one way at a time. Nothing in the app, its systems or its library is changed by this check.',
 'Robust means: no failed gate, grade B or better, and every axis measured. A system that makes no money on its own history has nothing to protect and is not robust.',
 V.complete?'Every axis was measured.':'Not measured: '+Object.entries(det).filter(([k,rows])=>(rows||[]).some(r=>r.error)).map(([k])=>LABEL[k]||k).join(', ')+'.',
 `Run time: ${int(D.seconds)} s.`].map(t=>`<li>${esc(t)}</li>`).join('');
'''
