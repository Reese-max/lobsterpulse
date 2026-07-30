#!/usr/bin/env node
const fs=require('fs'),path=require('path'),cp=require('child_process');
function fmt(n){return n>=1e9?(n/1e9).toFixed(1)+'B':n>=1e6?(n/1e6).toFixed(1)+'M':n>=1e3?(n/1e3).toFixed(1)+'K':String(n);}
function localDate(){const d=new Date(); const y=d.getFullYear(); const m=String(d.getMonth()+1).padStart(2,'0'); const day=String(d.getDate()).padStart(2,'0'); return `${y}-${m}-${day}`;}
const home=process.env.USERPROFILE||'C:/Users/Administrator';
let base={};
try{
  const out=cp.execFileSync('node',[path.join(home,'.lobsterpulse','scripts','claude-direct.js')],{timeout:12000,encoding:'utf8'});
  base=JSON.parse(out.trim().split('\n').pop());
}catch(e){base={ok:false,error:'claude-direct:'+e.message};}
let today={totalTokens:0,totalCost:0,date:'-'};
let ccusageCacheDate=null;
let ccusageCacheFresh=false;
try{
  const cache=JSON.parse(fs.readFileSync(path.join(home,'.lobsterpulse','ccusage-today.json'),'utf8'));
  const todayStr=localDate();
  ccusageCacheDate=cache.daily?.[cache.daily.length-1]?.date||null;
  const todayMatch=(cache.daily||[]).find(d=>d.date===todayStr);
  if(todayMatch){
    today=todayMatch;
    ccusageCacheFresh=true;
  }
}catch(e){}
const todayTokens=today.totalTokens||0;
const todayCost=today.totalCost||0;
const merged={
  ...base,
  basis:'provider_api',
  today_tokens:todayTokens>0?fmt(todayTokens):'N/A',
  today_cost:todayCost>0?'$'+todayCost.toFixed(2):'N/A',
  today_date:today.date||'N/A',
  ccusage_cache_date:ccusageCacheDate,
  ccusage_cache_fresh:ccusageCacheFresh,
};
console.log(JSON.stringify(merged));
