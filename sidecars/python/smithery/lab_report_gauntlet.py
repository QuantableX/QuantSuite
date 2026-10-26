"""The Gauntlet report: every indicator's Standard and per-track verdicts, one page."""
from .lab_report import page


def render(run, results):
    if results.get("kind") == "gauntlet" and results.get("source") != "run":
        from .lab_gauntlet import from_run
        results = from_run(results)
    data = {"run": {k: run.get(k) for k in ("id", "title", "kind", "status", "created_at", "finished_at")},
            **results}
    return page(run.get("title") or "Gauntlet", BODY, SCRIPT, data)


BODY = r'''
<header class="header">
 <div><div class="eyebrow" id="eyebrow">QuantScript · Gauntlet</div><h1 id="title"></h1><p class="lede" id="lede"></p><div class="chips" id="chips"></div></div>
 <div class="hero"><div class="label">Certified on all three tracks</div><div class="figure" id="hero"></div><div class="sub" id="hero-sub"></div></div>
</header>
<div class="tiles" id="tiles"></div>
<section class="panel" id="summary">
 <div class="panel-head"><div><div class="kicker">Certification</div><h2>Where the indicators pass</h2><div class="sub">A track passes with a score of at least 70, a permutation p-value of at most 0.10 and every mandatory check complete. The overall verdict is the worst of the three tracks.</div></div></div>
 <div class="split">
  <div><div class="table-wrap"><table class="data" aria-label="Certified indicators by track"><thead><tr><th>Certified</th><th class="num">1h</th><th class="num">4h</th><th class="num">1d</th><th class="num">All three</th></tr></thead><tbody id="counts"></tbody></table></div>
   <p class="small muted" style="margin-top:10px">Selected = the version kept for each track: optimized parameters when they won, otherwise the Standard parameters. Standard = one fixed parameter set on every track.</p></div>
  <div><h3>Overall score distribution · selected versions</h3><div id="histogram"></div><p class="small muted" id="histogram-note"></p></div>
 </div>
</section>
<section class="panel" id="results">
 <div class="panel-head"><div><div class="kicker">All results</div><h2>Every indicator on every track</h2><div class="sub">Select a name for the five test axes, walk-forward, parameters and the native evidence.</div></div>
  <div class="controls" style="margin:0"><button class="btn" id="csv-summary" type="button">Download CSV · indicators</button><button class="btn" id="csv-details" type="button">Download CSV · individual results</button></div></div>
 <div class="controls">
  <label class="field">Find an indicator<input id="search" type="search" placeholder="Name or key" autocomplete="off"></label>
  <label class="field">Version<select id="role"><option value="selected">Selected per track</option><option value="standard">Standard</option></select></label>
  <label class="field">Show<select id="filter"><option value="all">All indicators</option><option value="cert">Certified on all three tracks</option><option value="any">Certified on at least one track</option><option value="1h">Certified on 1h</option><option value="4h">Certified on 4h</option><option value="1d">Certified on 1d</option><option value="promoted">Optimized version selected</option><option value="incomplete">Incomplete evidence</option><option value="error">Errors</option></select></label>
  <label class="field">Sort by<select id="sort"><option value="overall">Overall score ↓</option><option value="1h">1h score ↓</option><option value="4h">4h score ↓</option><option value="1d">1d score ↓</option><option value="p">Worst p-value ↑</option><option value="name">Name A–Z</option></select></label>
 </div>
 <div class="statusline"><span id="count" aria-live="polite"></span><span>Scores from 0 to 100 · permutation p-value below each score · ✓ certified</span></div>
 <div class="table-wrap tall"><table class="data" id="table"><thead><tr><th class="num">#</th><th>Indicator</th><th class="num"><button data-sort="1h" type="button">1h</button></th><th class="num"><button data-sort="4h" type="button">4h</button></th><th class="num"><button data-sort="1d" type="button">1d</button></th><th class="num"><button data-sort="overall" type="button">Overall</button></th><th class="num"><button data-sort="p" type="button">Worst p</button></th><th>All three</th></tr></thead><tbody id="rows"></tbody></table></div>
</section>
<section class="panel" id="method">
 <div class="panel-head"><div><div class="kicker">Reading this report</div><h2>What the scores mean, and what they do not</h2></div></div>
 <div class="cols">
  <div><h3>Robustness, not profit</h3><p>A Gauntlet score combines five equally weighted axes: other assets, other exchanges, neighbouring parameters, other periods and Monte Carlo paths. It measures how reliably a signal holds up. It is not a portfolio return or an execution test.</p></div>
  <div><h3>The worst track decides</h3><p>Every indicator runs on 1h, 4h and 1d. The overall score is the lowest of the three, the overall p-value the highest. Overall certification needs all three tracks certified.</p></div>
  <div><h3>Missing is not zero</h3><p>Tracks without a report, incomplete axes and missing walk-forward records are shown as missing. Missing or failed evidence is never counted as a pass.</p></div>
 </div>
 <details class="method"><summary>Protocol, sources and limitations</summary><div id="protocol"></div></details>
</section>
<footer class="footer"><span id="foot-left"></span><span>One offline HTML file · all evidence embedded</span></footer>
<style>.split{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1.3fr);gap:28px;align-items:start}.split h3{margin-bottom:4px}@media(max-width:900px){.split{grid-template-columns:1fr}}
.cell{display:flex;flex-direction:column;align-items:flex-end}.cell .score.pass{color:var(--good-ink)}.name-btn{all:unset;cursor:pointer;font-weight:600;color:var(--ink)}.name-btn:hover{color:var(--accent);text-decoration:underline}.name-btn:focus-visible{outline:2px solid var(--accent)}
#table{min-width:900px}#table td:nth-child(2){width:34%}.flags{display:flex;gap:6px;flex-wrap:wrap;margin-top:4px}.axes{font-size:12px}.axes td,.axes th{white-space:nowrap;padding:8px 10px}.axes .meter{width:36px;margin-right:6px}.reasons{margin:6px 0 0;padding-left:18px;color:var(--ink-2);font-size:13px}</style>
'''


SCRIPT = r'''
const T=['1h','4h','1d'],AX=['asset','exchange','parameter','temporal','monte_carlo'],AXL={asset:'Asset',exchange:'Exchange',parameter:'Parameter',temporal:'Temporal',monte_carlo:'Monte Carlo'};
const {esc,num,int,$}=Q,S=DATA.summary||{},R=DATA.rows||[],run=DATA.run||{},isRun=DATA.source==='run';
const yes=b=>b?'<span class="badge pass">✓ Certified</span>':'<span class="badge fail">Uncertified</span>';
$('title').textContent=run.title||'Gauntlet';
$('lede').textContent=isRun?'New research candidates through the full native Gauntlet with walk-forward optimization on 1h, 4h and 1d. Standard parameters and the version selected for each track are shown side by side.':'The existing native Gauntlet evidence of every indicator in the library, compiled without a new test run. Standard parameters and the version selected for each track are shown side by side.';
const chips=[(isRun?'Research run':'Library snapshot')+' · '+String(run.created_at||'').slice(0,10),S.report_dates?'Reports '+S.report_dates[0]+' to '+S.report_dates[1]:'No native reports',...(S.protocols||[]).map(p=>'Seed '+p.seed+' · '+int(p.counts?.perm)+' permutations · '+int(p.counts?.boot)+' bootstraps · '+int(p.counts?.garch)+' GARCH paths'+(p.fast?' · fast smoke test':'')),...(S.evaluation_versions||[])];
$('chips').innerHTML=chips.map(c=>'<span class="chip">'+esc(c)+'</span>').join('');
const C=S.counts||{standard:{},selected:{}};
$('hero').textContent=int(C.selected.all);
$('hero-sub').innerHTML='of '+esc(Q.plural(S.indicators,'indicator'))+', selected versions<br>Standard parameters: '+esc(int(C.standard.all));
const anyCert=R.filter(r=>T.some(t=>r.selected[t]?.certified)).length;
const tiles=[['Indicators',int(S.indicators),isRun?'Candidates in this research run':'Base indicators in the library'],
 ['Best overall score',S.best?num(S.best.score,1):'—',S.best?S.best.name+' · '+S.best.key:'No complete indicator'],
 ['Certified on at least one track',int(anyCert),'Selected · 1h '+int(C.selected['1h'])+' · 4h '+int(C.selected['4h'])+' · 1d '+int(C.selected['1d'])],
 ['Native reports',int(S.reports),(S.incomplete_reports?.length?S.incomplete_reports.length+' incomplete':'All complete')+' · '+int(S.missing_walkforward)+' of '+int(S.track_versions)+' track versions without walk-forward']];
$('tiles').innerHTML=tiles.map(t=>'<div class="tile"><div class="label">'+esc(t[0])+'</div><div class="value">'+esc(t[1])+'</div><div class="sub">'+esc(t[2])+'</div></div>').join('');
$('counts').innerHTML=['selected','standard'].map(role=>'<tr><td>'+(role==='selected'?'Selected per track':'Standard')+'</td>'+T.map(t=>'<td class="num">'+int(C[role][t])+'</td>').join('')+'<td class="num"><b>'+int(C[role].all)+'</b></td></tr>').join('');
const bins=Array(20).fill(0);let missing=0;R.forEach(r=>{const s=r.overall.selected.score;if(s==null){missing++;return}bins[Math.min(19,Math.max(0,Math.floor(s/5)))]++});
const drawHist=()=>Q.columns($('histogram'),{values:bins,labels:bins.map((_,i)=>'Overall score '+i*5+'–'+(i*5+5)),short:i=>String(i*5),labelEvery:2,unit:v=>v===1?'indicator':'indicators',tip:i=>Q.plural(bins[i],'indicator')+' scored '+i*5+' to '+(i*5+5),marker:14,markerLabel:'Certification score 70',label:'Overall score distribution of the selected versions',height:200});
$('histogram-note').textContent='Overall = worst track. '+(missing?Q.plural(missing,'indicator')+' without all three tracks '+(missing===1?'is':'are')+' not counted. ':'')+'A score of 70 or more also needs p ≤ 0.10 to certify.';
let role='selected',filter='all',sort='overall',q='';
const metric=(r,k)=>k==='overall'?r.overall[role].score:k==='p'?r.overall[role].perm_p:r[role][k]?.score;
function cell(v){if(!v)return'<td class="num"><span class="muted">—</span><span class="p">no report</span></td>';return'<td class="num"><div class="cell"><span class="score'+(v.certified?' pass':'')+'">'+num(v.score,1)+(v.certified?' ✓':'')+'</span><span class="p">p '+num(v.perm_p,3)+'</span>'+(v.complete?'':'<span class="badge warn">Incomplete</span>')+'</div></td>'}
function visible(){const words=q.toLowerCase().split(/\s+/).filter(Boolean);return R.filter(r=>words.every(w=>(r.key+' '+r.name).toLowerCase().includes(w))).filter(r=>filter==='all'||filter==='cert'&&r.overall[role].certified||filter==='any'&&T.some(t=>r[role][t]?.certified)||T.includes(filter)&&r[role][filter]?.certified||filter==='promoted'&&T.some(t=>r.decision?.[t]==='promoted')||filter==='incomplete'&&['standard','selected'].some(x=>T.some(t=>r[x][t]&&!r[x][t].complete))||filter==='error'&&r.error)}
function draw(){const rs=visible();const dir=sort==='p'?1:-1;rs.sort((a,b)=>{if(sort==='name')return a.name.localeCompare(b.name)||a.key.localeCompare(b.key);const x=metric(a,sort),y=metric(b,sort);if(x==null&&y==null)return a.key.localeCompare(b.key);if(x==null)return 1;if(y==null)return-1;return(x-y)*dir||a.key.localeCompare(b.key)});
 $('count').textContent=int(rs.length)+' of '+Q.plural(R.length,'indicator')+' · '+(role==='standard'?'Standard':'Selected per track');
 $('rows').innerHTML=rs.map((r,i)=>{const o=r.overall[role],flags=[];if(r.error)flags.push('<span class="badge bad">Error</span>');if(['standard','selected'].some(x=>T.some(t=>r[x][t]&&!r[x][t].complete)))flags.push('<span class="badge warn">Incomplete evidence</span>');if(role==='selected'&&T.some(t=>r.decision?.[t]==='promoted'))flags.push('<span class="badge fail">Optimized on '+T.filter(t=>r.decision?.[t]==='promoted').join(', ')+'</span>');
  return'<tr><td class="num muted">'+(i+1)+'</td><td><button class="name-btn" type="button" data-key="'+esc(r.key)+'">'+esc(r.name)+'</button><div class="key">'+esc(r.key)+'</div>'+(flags.length?'<div class="flags">'+flags.join('')+'</div>':'')+(r.error?'<div class="small" style="color:var(--bad-ink)">'+esc(r.error)+'</div>':'')+'</td>'+T.map(t=>cell(r[role][t])).join('')+'<td class="num"><span class="score">'+num(o.score,1)+'</span>'+(o.worst?'<span class="p">worst '+o.worst+'</span>':'')+'</td><td class="num">'+num(o.perm_p,3)+'</td><td>'+yes(o.certified)+'</td></tr>'}).join('')||'<tr><td colspan="8" class="empty">No indicator matches these filters.</td></tr>';
 document.querySelectorAll('#table th button').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.sort===sort)))}
$('rows').addEventListener('click',e=>{const b=e.target.closest('.name-btn');if(b)detail(b.dataset.key)});
$('search').addEventListener('input',e=>{q=e.target.value;draw()});
$('role').addEventListener('change',e=>{role=e.target.value;draw()});
$('filter').addEventListener('change',e=>{filter=e.target.value;draw()});
$('sort').addEventListener('change',e=>{sort=e.target.value;draw()});
document.querySelectorAll('#table th button').forEach(b=>b.addEventListener('click',()=>{sort=b.dataset.sort;$('sort').value=sort;draw()}));
function axisCell(v){return'<td>'+(v==null?'<span class="muted">—</span>':'<span class="meter"><i style="width:'+Math.max(0,Math.min(100,v))+'%"></i></span>'+num(v,1))+'</td>'}
function detail(key){const r=R.find(x=>x.key===key);if(!r)return;let h=r.hypothesis?'<p class="muted">'+esc(r.hypothesis)+'</p>':'';
 if(r.error)h+='<p class="note">'+esc(r.error)+'</p>';
 h+='<h3>Verdicts by track · five axes</h3><div class="table-wrap"><table class="data axes"><thead><tr><th>Version</th><th>Track</th><th class="num">Score</th><th>Grade</th><th class="num">p-value</th><th>Verdict</th>'+AX.map(a=>'<th>'+AXL[a]+'</th>').join('')+'</tr></thead><tbody>';
 for(const x of ['standard','selected'])for(const t of T){const v=r[x][t];h+='<tr><td>'+(x==='standard'?'Standard':'Selected · '+esc(r.versions?.[t]||'Standard'))+'</td><td>'+t+'</td>'+(v?'<td class="num">'+num(v.score,1)+'</td><td title="'+esc(v.grade||'')+'">'+esc(String(v.grade||'—').split(' ')[0])+(/capped/.test(v.grade||'')?' <span class="badge fail">capped</span>':'')+'</td><td class="num">'+num(v.perm_p,4)+'</td><td>'+yes(v.certified)+(v.complete?'':' <span class="badge warn">Incomplete</span>')+'</td>'+AX.map(a=>axisCell(v.axes?.[a])).join(''):'<td colspan="'+(4+AX.length)+'" class="muted">No report for this track</td>')+'</tr>'}
 h+='</tbody></table></div>';
 const reasons=[];for(const x of ['standard','selected'])for(const t of T){const v=r[x][t];if(v&&!v.certified&&v.reasons?.length)reasons.push('<li><b>'+(x==='standard'?'Standard':'Selected')+' '+t+':</b> '+esc(v.reasons.join('; '))+'</li>')}
 if(reasons.length)h+='<h3>Why tracks did not certify</h3><ul class="reasons">'+[...new Set(reasons)].join('')+'</ul>';
 h+='<h3>Walk-forward · selected version per track</h3><div class="table-wrap"><table class="data"><thead><tr><th>Track</th><th>Decision</th><th class="num">WFE</th><th class="num">Out-of-sample Sharpe</th><th class="num">In-sample Sharpe</th><th class="num">Folds</th><th>Series</th></tr></thead><tbody>'+T.map(t=>{const w=r.wf?.[t];return'<tr><td>'+t+'</td><td>'+esc(({promoted:'Optimized parameters selected',standard_retained:'Standard retained',general_retained:'General optimization retained'})[r.decision?.[t]]||r.decision?.[t]||'—')+'</td>'+(w?'<td class="num">'+num(w.wfe,3)+'</td><td class="num">'+num(w.oos_sharpe,3)+'</td><td class="num">'+num(w.is_sharpe_mean,3)+'</td><td class="num">'+int(w.folds)+'</td><td class="key">'+esc(w.series||'')+'</td>':'<td colspan="5" class="muted">No walk-forward record</td>')+'</tr>'}).join('')+'</tbody></table></div><p class="small muted">Walk-forward evaluates the optimization candidate; when the Standard parameters were retained, its efficiency describes the rejected candidate.</p>';
 const names=[...new Set([...Object.keys(r.params?.standard||{}),...T.flatMap(t=>Object.keys(r.params?.[t]||{}))])];
 const pv=v=>v==null?'—':typeof v==='number'?num(v,Number.isInteger(v)?0:4):esc(JSON.stringify(v));
 h+='<h3>Parameters</h3>'+(names.length?'<div class="table-wrap"><table class="data"><thead><tr><th>Parameter</th><th class="num">Standard</th>'+T.map(t=>'<th class="num">Selected '+t+'</th>').join('')+'</tr></thead><tbody>'+names.map(n=>'<tr><td class="key">'+esc(n)+'</td><td class="num">'+pv(r.params.standard?.[n])+'</td>'+T.map(t=>'<td class="num">'+pv(r.params?.[t]?.[n])+'</td>').join('')+'</tr>').join('')+'</tbody></table></div>':'<p class="muted">No tunable parameters.</p>');
 const reps=[...new Set(['standard','selected'].flatMap(x=>T.map(t=>r[x][t]?.report)).filter(Boolean))];
 h+='<h3>Native evidence</h3>'+(reps.length?reps.map(n=>'<details class="method"><summary>'+esc(n)+'</summary><pre class="evidence">'+esc(JSON.stringify(DATA.reports?.[n]??'Report file not found',null,2))+'</pre></details>').join(''):'<p class="muted">No native reports.</p>');
 Q.sheet(r.key,r.name,(r.script?r.script+' · ':'')+'warm-up '+int(r.warmup)+' bars',h)}
const flat=r=>{const o={indicator:r.key,name:r.name,error:r.error||null};for(const x of ['standard','selected']){for(const t of T){const v=r[x][t]||{},p=x+'_'+t;o[p+'_score']=v.score??null;o[p+'_perm_p']=v.perm_p??null;o[p+'_certified']=v.certified??null;o[p+'_grade']=v.grade??null;o[p+'_complete']=v.complete??null;o[p+'_report']=v.report??null;o[p+'_record_date']=v.date??null}const ov=r.overall[x];o[x+'_minimum_score']=ov.score;o[x+'_worst_perm_p']=ov.perm_p;o[x+'_certified_all_tracks']=ov.certified;o[x+'_all_axes_complete']=ov.complete}
 for(const t of T){const w=r.wf?.[t];o['decision_'+t]=r.decision?.[t]??null;o['wfe_'+t]=w?.wfe??null;o['wf_oos_sharpe_'+t]=w?.oos_sharpe??null;o['wf_folds_'+t]=w?.folds??null}
 o.standard_params_json=JSON.stringify(r.params?.standard??null);for(const t of T)o['selected_'+t+'_params_json']=JSON.stringify(r.params?.[t]??null);return o};
const stem=(isRun?'gauntlet-research-':'gauntlet-all-indicators')+(isRun?String(run.created_at||'').slice(0,10):'');
$('csv-summary').onclick=()=>{const rows=R.map(flat);Q.download(stem+'.csv',Q.csv(Object.keys(rows[0]||{indicator:''}),rows))};
$('csv-details').onclick=()=>{const rows=[];for(const r of R)for(const x of ['standard','selected'])for(const t of T){const v=r[x][t];if(!v)continue;const rep=DATA.reports?.[v.report]||{},w=x==='selected'?r.wf?.[t]:null;rows.push({indicator:r.key,role:x,timeframe:t,version:x==='standard'?r.key:(r.versions?.[t]||r.key),score:v.score,native_score:rep.score??null,perm_p:v.perm_p,certified:v.certified,complete:v.complete,grade:v.grade,...Object.fromEntries(AX.map(a=>['axis_'+a,v.axes?.[a]??null])),params_json:JSON.stringify(x==='standard'?r.params?.standard:r.params?.[t]),wfe:w?.wfe??null,wf_oos_sharpe:w?.oos_sharpe??null,wf_is_sharpe_mean:w?.is_sharpe_mean??null,wf_folds:w?.folds??null,selection_decision:x==='selected'?r.decision?.[t]??null:null,reasons_json:JSON.stringify(v.reasons||[]),record_date:v.date,report:v.report,evaluation_version:v.evaluation_version,seed:rep.seed??null,permutations:rep.counts?.perm??null,bootstraps:rep.counts?.boot??null,garch:rep.counts?.garch??null,fast:rep.fast??null,primary:rep.primary??null,primary_sha256:rep.primary_sha256??null,code_sha256:rep.code_sha256??null})}Q.download(stem+'-details.csv',Q.csv(Object.keys(rows[0]||{indicator:''}),rows))};
const P=[];P.push('<p>'+(isRun?'Fresh evaluation of this research run: frozen engine, private library copy and market data, then the unchanged native optimizer.':'Compiled from the library\'s registry and the native JSON reports in the Forge vault. Nothing was re-tested; historical reports may use different data snapshots (their hashes are in the details CSV).')+'</p>');
P.push('<p>'+int(S.reports)+' unique native reports, '+int(S.complete_reports)+' with all five axes. Standard and selected versions may reference the same report. '+int(S.track_versions-S.missing_walkforward)+' of '+int(S.track_versions)+' track versions carry a walk-forward record.</p>');
if(S.incomplete_reports?.length)P.push('<p><b>Incomplete reports:</b> '+S.incomplete_reports.map(esc).join('; ')+'. Their available scores remain visible; they are uncertified.</p>');
P.push('<p>Evaluation versions: '+(S.evaluation_versions||[]).map(esc).join(', ')+'. Gauntlet scores measure signal robustness. They are not portfolio returns or execution tests, and a high score is no promise of future performance.</p>');
$('protocol').innerHTML=P.join('');
$('foot-left').textContent=(isRun?'QuantScript research run':'QuantScript library')+' · '+int(S.indicators)+' indicators · generated '+String(run.finished_at||run.created_at||'').slice(0,10);
draw();drawHist();const watch=Q.onResize(drawHist);watch($('histogram'));
'''
