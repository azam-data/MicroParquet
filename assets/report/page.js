(function(){
var D=JSON.parse(document.getElementById('answers').textContent);
var L=JSON.parse(document.getElementById('looks').textContent);
var DS=JSON.parse(document.getElementById('designs').textContent);
var TH=JSON.parse(document.getElementById('themes').textContent);
var root=document.documentElement, dark=false, errors=[];
try{dark=matchMedia('(prefers-color-scheme: dark)').matches}catch(e){}
window.addEventListener('error',function(e){errors.push(String(e.message))});
echarts.registerTheme('mpl',TH.light);echarts.registerTheme('mpd',TH.dark);
var PS=D.pages.map(function(p){return {picks:p.filters.map(function(){return null}),cross:null,drill:{},open:{}}});
var cur=0, charts=[], viewed={}, points={};
function el(t,c,x){var e=document.createElement(t);if(c)e.className=c;if(x!=null)e.textContent=x;return e}
function group(n){var s=String(Math.abs(n)),p=s.split('.');p[0]=p[0].replace(/\B(?=(\d{3})+(?!\d))/g,',');return (n<0?'−':'')+p.join('.')}
function fmt(v,dec){return v==null?'':group(Number(v.toFixed(dec==null?2:dec)))}
function same(a,b){return JSON.stringify(a)===JSON.stringify(b)}
function wanted(pi,it){var ps=PS[pi];return D.pages[pi].filters.map(function(f,i){return it.filters.indexOf(i)>=0?ps.picks[i]:null})}
function find(pi,it){
  var ps=PS[pi],w=wanted(pi,it),i,a;
  if(ps.cross&&ps.cross.src!==it.id){
    for(i=0;i<it.cross.length;i++){var x=it.cross[i];if(x.k[0]===ps.cross.src&&x.k[1]===ps.cross.value&&same(x.a.p,w))return {a:x.a,path:[],crossed:true}}
  }
  var path=ps.cross&&ps.cross.src!==it.id?[]:(ps.drill[it.id]||[]);
  for(i=0;i<it.answers.length;i++){a=it.answers[i];if(same(a.p,w)&&same(a.d,path))return {a:a,path:path,crossed:false}}
  return null;
}
function dimmed(pi,it,cat){var c=PS[pi].cross;return c&&c.src===it.id&&c.value!==cat}
function chartOption(pi,it,a){
  var lk=it.look,u=a.mu||[it.unit],pct=lk.st==='p',M=a.m.length||1,S=a.s.length||1,tot=[];
  var right=u.some(function(x){return x!==u[0]});
  var ru=right?u.filter(function(x){return x!==u[0]})[0]:'';
  var st=TH.status;
  function unitOf(m){return u[m]||''}
  function fm(v,m){return v==null?'':fmt(v,u[m]==='%'?1:(u[m]===''?0:2))+(unitOf(m)?' '+unitOf(m):'')}
  if(lk.ct==='waterfall'){
    var th=TH[dark?'dark':'light'],base=th.color[0];
    return {tooltip:{trigger:'item',formatter:function(p){var w=a.wf[p.dataIndex];return w[0]+'<br>'+(w[1]==='d'?'Change ':'')+fmt(w[2])+' '+u[0]+(w[1]==='d'?'<br>Level '+fmt(w[5])+' '+u[0]:'')}},
      grid:{left:60,right:20,top:30,bottom:24,containLabel:true},
      xAxis:{type:'category',data:a.wf.map(function(w){return w[0]}),axisLabel:{interval:0,hideOverlap:true}},
      yAxis:{type:'value',name:u[0],axisLabel:{formatter:function(v){return fmt(v)}}},
      series:[{type:'custom',data:a.wf.map(function(w,i){return {value:[i,w[3],w[4]],itemStyle:{color:w[1]==='d'?(w[2]>=0?st.good:st.critical):base}}}),
        renderItem:function(p,api){var x=api.value(0),lo=api.coord([x,api.value(1)]),hi=api.coord([x,api.value(2)]),w=api.size([1,0])[0]*0.6;
          return {type:'rect',shape:{x:lo[0]-w/2,y:Math.min(hi[1],lo[1]),width:w,height:Math.max(1,Math.abs(lo[1]-hi[1]))},style:api.style()}},encode:{x:0,y:[1,2]}}]};
  }
  if(lk.ct==='heatmap'){
    var hm=a.hm,data=[],pos=[];
    hm.cells.forEach(function(r,ri){r.forEach(function(v,ci){if(v!=null){data.push([ci,ri,v]);if(v>0)pos.push(v)}})});
    var lo=pos.length?Math.min.apply(null,pos):0,hi=pos.length?Math.max.apply(null,pos):1;
    return {tooltip:{position:'top',formatter:function(p){return hm.rows[p.value[1]]+' · '+hm.cols[p.value[0]]+': '+fmt(p.value[2])+' '+u[0]}},
      grid:{left:60,right:20,top:20,bottom:50,containLabel:true},
      xAxis:{type:'category',data:hm.cols,axisLabel:{interval:0,hideOverlap:true}},yAxis:{type:'category',data:hm.rows,inverse:true},
      visualMap:{type:'continuous',min:lo,max:hi===lo?lo+1:hi,show:false,dimension:2,inRange:{color:TH.seq[dark?'dark':'light']},outOfRange:{color:getComputedStyle(root).getPropertyValue('--grid').trim()||'#e1e0d9'}},
      series:[{type:'heatmap',data:data,label:{show:true,formatter:function(p){return fmt(p.value[2])}}}]};
  }
  if(lk.ct==='hist'){
    var h=a.hg,names=h.counts.map(function(n,i){return fmt(h.edges[i])+' – '+fmt(h.edges[i+1])});
    return {tooltip:{trigger:'axis',axisPointer:{type:'shadow'},valueFormatter:function(v){return fmt(v,0)}},grid:{left:60,right:20,top:30,bottom:24,containLabel:true},
      xAxis:{type:'category',data:names,axisLabel:{interval:0,hideOverlap:true}},yAxis:{type:'value',name:'Count',axisLabel:{formatter:function(v){return fmt(v,0)}}},
      series:[{type:'bar',barCategoryGap:'5%',data:h.counts}]};
  }
  if(lk.ct==='donut'){
    return {tooltip:{trigger:'item',valueFormatter:function(v){return fm(v,0)}},legend:{bottom:0,type:'scroll'},
      series:[{type:'pie',radius:['45%','70%'],label:{formatter:'{b}'},data:a.c.map(function(c,i){return {name:c,value:a.v[0][i],itemStyle:{opacity:dimmed(pi,it,c)?0.35:1}}})}]};
  }
  if(pct){for(var m0=0;m0<M;m0++){tot[m0]=a.c.map(function(c,i){var t=0;for(var j0=0;j0<S;j0++)t+=a.v[m0*S+j0][i]||0;return t})}}
  var ser=[];
  for(var m=0;m<M;m++)(function(m){
    for(var j=0;j<S;j++)(function(j){
      var nm=M>1?((a.s[j]?a.s[j]+' · ':'')+a.m[m]):(a.s[j]||a.m[0]);
      var o={name:nm,type:lk.ct==='bar'?'bar':'line',yAxisIndex:u[m]!==u[0]?1:0,data:a.v[m*S+j].map(function(v,i){
        var x=pct&&v!=null?(tot[m][i]?v/tot[m][i]*100:null):v;
        return dimmed(pi,it,a.c[i])?{value:x,itemStyle:{opacity:0.35}}:x})};
      if(lk.st!=='g')o.stack='t'+m;
      if(lk.ct==='area')o.areaStyle={};
      if(lk.ct==='bar'&&a.s.length<=1&&M===1)o.barMaxWidth=60;
      ser.push(o)})(j)})(m);
  var cat={type:'category',data:a.c,axisLabel:{interval:0,hideOverlap:true}};
  var val={type:'value',axisLabel:{formatter:function(v){return fmt(v)}},name:pct?'%':u[0]};
  var vals=[val];if(right)vals.push({type:'value',axisLabel:{formatter:function(v){return fmt(v)}},name:ru,splitLine:{show:false}});
  return {tooltip:{trigger:'axis',axisPointer:{type:'shadow'},valueFormatter:function(v){return v==null?'':fmt(v)+(pct?' %':'')}},
    legend:(a.s.length>1||M>1)?{bottom:0,type:'scroll'}:undefined,grid:{left:60,right:right?60:20,top:30,bottom:(a.s.length>1||M>1)?40:24,containLabel:true},
    xAxis:lk.hz?vals[0]:cat,yAxis:lk.hz?cat:(vals.length>1?vals:vals[0]),series:ser};
}
function menu(x,y,items){
  closeMenu();var m=el('div','mp-menu');m.id='mp-menu';m.style.left=x+'px';m.style.top=y+'px';
  items.forEach(function(t){var b=el('button',null,t[0]);b.onclick=function(){closeMenu();t[1]()};m.appendChild(b)});
  document.body.appendChild(m);
}
function closeMenu(){var m=document.getElementById('mp-menu');if(m)m.remove()}
document.addEventListener('click',closeMenu);
function toggleCross(pi,it,cat){
  var ps=PS[pi];
  if(ps.cross&&ps.cross.src===it.id&&ps.cross.value===cat)ps.cross=null;else ps.cross={src:it.id,value:cat,label:it.dim+' = '+cat};
  ps.drill={};draw();
}
function drillTo(pi,it,path,cat){
  var next=path.concat([cat]);
  var ok=it.answers.some(function(a){return same(a.d,next)&&same(a.p,wanted(pi,it))});
  if(!ok)return false;PS[pi].drill[it.id]=next;draw();return true;
}
function hasDrill(pi,it,path,cat){var n=path.concat([cat]),w=wanted(pi,it);return it.answers.some(function(a){return same(a.d,n)&&same(a.p,w)})}
function drawChart(pi,it,f,body){
  var a=f.a,lk=it.look;
  if(lk.ct==='table'){body.appendChild(el('div','mp-note','This chart is shown as a table here: the file does not draw this chart type.'));body.appendChild(flat(a.c,a.s.length?a.s:[a.m],a.v,true));return}
  var d=el('div','mp-chart');body.appendChild(d);
  var c=echarts.init(d,dark?'mpd':'mpl',{renderer:'canvas'});charts.push(c);
  var o=chartOption(pi,it,a);o.animation=false;c.setOption(o);
  points[it.id]=function(i){var s=a.s.length?0:0;var v=a.v[s][i];var px=c.convertToPixel({seriesIndex:0},lk.hz?[v,a.c[i]]:[a.c[i],v]);var r=d.getBoundingClientRect();
    if(lk.ct==='donut')return null;
    var base=c.convertToPixel({seriesIndex:0},lk.hz?[v/2,a.c[i]]:[a.c[i],v/2]);return {x:r.left+base[0],y:r.top+base[1]}};
  var catOf=function(p){return lk.ct==='donut'?p.name:lk.ct==='waterfall'?a.wf[p.dataIndex][0]:(p.name!=null&&p.name!==''?p.name:a.c[p.dataIndex])};
  c.on('click',function(p){
    if(f.crossed)return;
    var cat=catOf(p);if(f.path.length===0&&it.linked&&lk.ct!=='waterfall'&&lk.ct!=='heatmap'&&lk.ct!=='hist')toggleCross(pi,it,cat);
  });
  c.on('contextmenu',function(p){
    p.event.event.preventDefault();var cat=catOf(p),e=p.event.event,ent=[];
    if(f.path.length===0&&it.linked)ent.push(['Filter the page by this',function(){toggleCross(pi,it,cat)}]);
    if(hasDrill(pi,it,f.path,cat))ent.push(['Drill down',function(){drillTo(pi,it,f.path,cat)}]);
    if(ent.length)menu(e.clientX,e.clientY,ent);
  });
}
function flat(rows,heads,vals,byCol){
  var t=el('table','mp-table'),h=el('tr');h.appendChild(el('th',null,''));
  heads.forEach(function(x){h.appendChild(el('th',null,x))});t.appendChild(h);
  rows.forEach(function(r,i){var tr=el('tr');tr.appendChild(el('td',null,r));
    heads.forEach(function(x,j){tr.appendChild(el('td',null,fmt(vals[j][i])))});t.appendChild(tr)});
  return t;
}
function drawPivot(pi,it,f,body){
  var a=f.a,rows=a.r,ps=PS[pi],open=ps.open[it.id]||(ps.open[it.id]={});
  var nested=rows.length&&rows[0].indexOf('\u001f')>=0;
  var t=el('table','mp-table'),h=el('tr');h.appendChild(el('th',null,''));a.h.forEach(function(x){h.appendChild(el('th',null,x))});t.appendChild(h);
  var lv=rows.map(function(r){return nested?Number(r.split('\u001f')[0]):0});
  var okAt=[];
  rows.forEach(function(r,i){
    var label=nested?r.slice(r.indexOf('\u001f')+1):r;if(nested&&label==='')return;
    var l=lv[i],vis=l===0||!!okAt[l-1];
    okAt[l]=vis&&!!open[i];okAt.length=l+1;
    if(!vis)return;
    var tr=el('tr');tr.setAttribute('data-row',i);
    var td=el('td');td.style.paddingLeft=(10+l*16)+'px';
    var kids=nested&&i+1<rows.length&&lv[i+1]>l;
    if(kids){var b=el('button','mp-tog',open[i]?'\u25be':'\u25b8');b.setAttribute('aria-label',(open[i]?'Collapse ':'Expand ')+label);b.onclick=function(){open[i]=!open[i];draw()};td.appendChild(b)}
    else if(nested)td.appendChild(el('span',null,'\u00a0\u00a0\u00a0'));
    td.appendChild(document.createTextNode(label));tr.appendChild(td);
    a.h.forEach(function(x,j){tr.appendChild(el('td',null,fmt(a.v[j][i])))});t.appendChild(tr);
  });
  body.appendChild(t);
}
function drawKpi(pi,it,f,body){
  var a=f.a,k=a.k,w=el('div','mp-kpi'),st=TH.status;
  if(!k){body.appendChild(el('div','mp-err',a.x||'No value.'));return}
  var big=el('div','mp-big',k.val);big.setAttribute('data-kpi','value');
  var tc={amber:st.warning,red:st.critical,green:st.good}[k.thr];if(tc)big.style.color=tc;
  w.appendChild(big);
  if(k.chg){var cc=el('div','mp-chg',(k.arrow?k.arrow+' ':'')+k.chg);cc.setAttribute('data-kpi','change');
    cc.style.color=k.tone==='good'?st.good:k.tone==='bad'?st.critical:'var(--ink2)';
    cc.title=k.words?k.words+' against the earlier period':'';w.appendChild(cc);
    if(k.words)w.appendChild(el('div','mp-note',k.words+' against the earlier period'))}
  if(k.why)w.appendChild(el('div','mp-note',k.why));
  if(k.per)w.appendChild(el('div','mp-note',k.per));
  var sp=k.spark.filter(function(x){return x!=null});
  if(sp.length>1){var mn=Math.min.apply(null,sp),mx=Math.max.apply(null,sp),n=k.spark.length,pts=[];
    k.spark.forEach(function(x,i){if(x!=null)pts.push((i/(n-1)*100).toFixed(1)+','+(mx===mn?10:20-(x-mn)/(mx-mn)*20).toFixed(1))});
    var s=document.createElementNS('http://www.w3.org/2000/svg','svg');s.setAttribute('viewBox','0 -2 100 24');s.setAttribute('width','120');s.setAttribute('height','28');
    var pl=document.createElementNS('http://www.w3.org/2000/svg','polyline');pl.setAttribute('points',pts.join(' '));pl.setAttribute('fill','none');pl.setAttribute('stroke','currentColor');pl.setAttribute('stroke-width','1.5');s.appendChild(pl);w.appendChild(s)}
  w.setAttribute('aria-label',k.alt||'');
  body.appendChild(w);
}
function drawStatic(it,f,body){
  var a=f.a;
  if(a.tx){var d=el('div','mp-text mp-text-'+a.tx.z,a.tx.t);d.style.textAlign=a.tx.a;if(a.tx.b)d.style.fontWeight='600';body.appendChild(d)}
  else if(a.im){var g=el('img','mp-img');g.src=a.im.src;g.alt=a.im.alt;body.appendChild(g)}
}
function drawItem(pi,it,k,wrap){
  var ps=PS[pi],f=find(pi,it),box=el('div','mp-item'),r=L.pages[pi].rects[k],H=L.pages[pi].h;
  box.setAttribute('data-item',it.id);
  box.style.left=(r[0]/1000*100)+'%';box.style.top=(r[1]/H*100)+'%';box.style.width=(r[2]/1000*100)+'%';box.style.height=(r[3]/H*100)+'%';
  if(it.kind==='text'||it.kind==='image')box.className+=' mp-static';
  else box.appendChild(el('div','mp-ititle',it.title));
  var sub=el('div','mp-sub');box.appendChild(sub);
  var body=el('div','mp-body');box.appendChild(body);wrap.appendChild(box);
  if(!f){body.appendChild(el('div','mp-err','This combination is not in the file.'));return}
  viewed[it.id]={kind:it.kind,picks:f.a.p,path:f.path,crossed:f.crossed,a:f.a};
  var path=f.path;
  if(path.length){
    var back=el('button',null,'← Back');back.onclick=function(){ps.drill[it.id]=path.slice(0,-1);draw()};sub.appendChild(back);
    var c=el('button',null,'All');c.onclick=function(){ps.drill[it.id]=[];draw()};sub.appendChild(c);
    path.forEach(function(x,i){sub.appendChild(document.createTextNode(' › '));
      if(i<path.length-1){var b=el('button',null,x);b.onclick=function(){ps.drill[it.id]=path.slice(0,i+1);draw()};sub.appendChild(b)}else sub.appendChild(document.createTextNode(x))});
    sub.appendChild(document.createTextNode(' · '+it.title+' › '+path.join(' › ')));
  }
  else if(ps.cross&&ps.cross.src===it.id)sub.textContent='● Selected: '+ps.cross.value;
  else if(ps.cross&&!it.linked)sub.textContent='Not linked to clicks';
  if(it.kind==='text'||it.kind==='image'){drawStatic(it,f,body);return}
  if(it.kind==='chart'){if(f.a.x){body.appendChild(el('div','mp-err',f.a.x))}else drawChart(pi,it,f,body)}
  else if(it.kind==='pivot')drawPivot(pi,it,f,body);
  else drawKpi(pi,it,f,body);
  if(f.a.x&&it.kind!=='chart')body.appendChild(el('div','mp-err',f.a.x));
  (f.a.n||[]).forEach(function(n){body.appendChild(el('div','mp-note',n))});
}
function draw(){
  charts.forEach(function(c){c.dispose()});charts=[];viewed={};points={};
  var app=document.getElementById('app');app.textContent='';
  var wrap=el('div','mp-wrap');app.appendChild(wrap);
  var top=el('div','mp-top');top.appendChild(el('h1',null,D.title));
  var tb=el('button','mp-btn',dark?'Light':'Dark');tb.setAttribute('data-act','theme');tb.onclick=function(){dark=!dark;root.setAttribute('data-theme',dark?'dark':'light');draw()};top.appendChild(tb);wrap.appendChild(top);
  var tabs=el('div','mp-tabs');tabs.setAttribute('role','tablist');
  D.pages.forEach(function(p,i){var b=el('button','mp-tab',p.title);b.setAttribute('role','tab');b.setAttribute('aria-selected',i===cur?'true':'false');b.onclick=function(){cur=i;draw()};tabs.appendChild(b)});
  if(D.pages.length>1)wrap.appendChild(tabs);
  var pg=D.pages[cur],ps=PS[cur],fl=el('div','mp-filters');
  pg.filters.forEach(function(f,i){
    var lab=el('label',null,f.label+' ');var s=el('select');s.setAttribute('data-filter',i);
    var o=el('option',null,'All');o.value='';s.appendChild(o);
    f.values.forEach(function(v){var o=el('option',null,v);o.value=v;if(ps.picks[i]===v)o.selected=true;s.appendChild(o)});
    s.onchange=function(){ps.picks[i]=s.value===''?null:s.value;ps.drill={};draw()};lab.appendChild(s);fl.appendChild(lab)});
  if(ps.cross){var ch=el('button','mp-chip',ps.cross.dimLabel||'');ch.textContent=ps.cross.label+' ×';ch.setAttribute('data-chip','cross');ch.onclick=function(){ps.cross=null;draw()};fl.appendChild(ch)}
  wrap.appendChild(fl);
  var page=el('div','mp-page');page.style.aspectRatio='1000 / '+L.pages[cur].h;
  var bg=DS[pg.id];if(bg)page.style.backgroundImage='url('+bg+')';
  wrap.appendChild(page);
  pg.items.forEach(function(it,k){drawItem(cur,it,k,page)});
}
window.addEventListener('resize',function(){charts.forEach(function(c){c.resize()})});
window.__mp={errors:errors,view:function(id){return viewed[id]},point:function(id,i){return points[id]&&points[id](i)},
  state:function(){return JSON.parse(JSON.stringify(PS[cur]))},drawn:function(){return true}};
draw();
})();
