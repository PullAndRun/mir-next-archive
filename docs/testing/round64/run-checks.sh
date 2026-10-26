#!/usr/bin/env bash
# R64 targeted checks, not the Lead gate. No ci.ps1 or full build.
set -uo pipefail
export PYTHONUTF8=1 PYTHONIOENCODING=utf-8
cd "$(git rev-parse --show-toplevel)" || exit 2
verify() {
node - "$1" <<'JS'
const fs=require('fs'), cp=require('child_process'), crypto=require('crypto');
const mode=process.argv[2], sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const root='docs/testing/round64/', head='063c6f609e2ee61fc4d3cd8c5d322dcc32c49ce2';
function assert(ok,msg){if(!ok)throw Error(msg)}
function git(args){return cp.execFileSync('git',args,{stdio:['ignore','pipe','ignore']})}
function artifact(path){const b=fs.readFileSync(path);let blob=null;try{blob=sha(git(['show',`HEAD:${path}`]))}catch{}return {path,size:b.length,lines:b.toString('utf8').split('\n').length-(b.at(-1)===10?1:0),sha256:sha(b),git_blob_sha256:blob,tracked:git(['ls-files','--',path]).length>0}}
const paths=[root+'README.md',root+'run-checks.sh',root+'gen-tree-hashes.sh',root+'commit-tree-hashes.txt',root+'round64-c-evidence.md','target/round64/gate/attempt-1.log','target/round64/gate/attempt-1.meta.txt','crates/mir-client/src/net.rs','crates/mir-client/src/app.rs','crates/mir-client/src/ui/projectile.rs','crates/mir-client/tests/round64_event_reclaim.rs','crates/mir-client/tests/round64_projectile_ack.rs','docs/client.md','docs/roadmap.md','docs/process/round64-plan.md','docs/process/round64-menxia-review.md','docs/process/round64-shangshu-dispatch.md',root+'round64-final-audit.md'];
const gate={head,exit:0,log_bytes:205352,log_sha256:'3e228cef3ce730eac76ce936679b8557308d8d4f20ed1d766dbfb7a0b92b2604',utc_start:'2026-10-26T17:31:43Z',utc_end:'2026-10-26T17:35:16Z'};
function logCheck(b){assert(b.length===gate.log_bytes,'gate log bytes');assert(sha(b)===gate.log_sha256,'gate log sha256')}
function metaCheck(text){const m=Object.fromEntries(text.trim().split(/\r?\n/).map(x=>{let i=x.indexOf('=');return [x.slice(0,i),x.slice(i+1)]}));for(const [k,v] of Object.entries({gate_head:head,gate_head_short:'063c6f6',exit_code:'0',utc_start:gate.utc_start,utc_end:gate.utc_end,log_bytes:'205352',log_sha256:gate.log_sha256,items_ok:'12',items_fail:'0',log_path:'target/round64/gate/attempt-1.log'}))assert(m[k]===v,'meta '+k);assert(m.operator==='lead (唯一门禁执行者)','operator')}
function manifestCheck(d){assert(d.gate_head==='063c6f6','gate_head');assert(JSON.stringify(d.gate)===JSON.stringify(gate),'gate manifest');assert(d.artifacts.length===18,'artifact count');assert(JSON.stringify(d.artifacts)===JSON.stringify(paths.map(artifact)),'artifact bytes/lines/hash/blob/tracked mismatch')}
function forbidden(t){return /R64\s*(?:已通过|完整通过|PASS)|(?:事件表|缓存)(?:已限容|有界)|生产多消费者已支持|已实现服务端\s*ack/i.test(t)}
function additions(){let t=git(['diff','--unified=0','--','docs/client.md','docs/roadmap.md']).toString();t+='\n'+fs.readFileSync(root+'README.md','utf8');return t.split('\n').filter(x=>!x.startsWith('+++')&&(x.startsWith('+')||!x.startsWith('-'))).join('\n')}
function reject(fn){let failed=false;try{fn()}catch{failed=true}assert(failed,'negative control failed to reject')}
if(mode==='refresh'){fs.writeFileSync(root+'final-manifest.json',JSON.stringify({schema:1,gate_head:'063c6f6',stage:'门禁通过；独立复跑绿；终审待刑部/尚书/门下，未 PASS',gate,artifacts:paths.map(artifact)},null,2)+'\n')}
else if(mode==='gate')logCheck(fs.readFileSync('target/round64/gate/attempt-1.log'));
else if(mode==='meta')metaCheck(fs.readFileSync('target/round64/gate/attempt-1.meta.txt','utf8'));
else if(mode==='manifest')manifestCheck(JSON.parse(fs.readFileSync(root+'final-manifest.json','utf8')));
else if(mode==='claims')assert(!forbidden(additions()),'positive forbidden claim');
else if(mode==='attributes'){const diff=git(['diff','063c6f6^','--unified=0','--','crates/mir-client']).toString();assert(!diff.split('\n').some(x=>x.startsWith('+')&&!x.startsWith('+++')&&/#\s*\[\s*(ignore|allow)\b/.test(x)),'added ignore/allow');assert(git(['diff','063c6f6^','--','crates/mir-protocol']).length===0,'wire diff')}
else if(mode==='negative'){reject(()=>logCheck(Buffer.from('bad')));const bad=fs.readFileSync('target/round64/gate/attempt-1.log');bad[0]^=1;reject(()=>logCheck(bad));reject(()=>metaCheck('exit_code=1'));const good=JSON.parse(fs.readFileSync(root+'final-manifest.json','utf8'));manifestCheck(good);for(let i=0;i<18;i++){const d=JSON.parse(JSON.stringify(good));d.artifacts[i].sha256='bad';reject(()=>manifestCheck(d))}reject(()=>manifestCheck({gate_head:'wrong'}));assert(forbidden('事件表已限容'),'claim negative control');assert(/#\s*\[\s*(ignore|allow)\b/.test('#[ignore]'),'attribute negative control'); console.log('bad bytes/same-length SHA/meta/18 artifact hashes/claim/attribute rejected')}
else throw Error('unknown verifier '+mode);
console.log(mode+' OK');
JS
}
if [[ ${1:-} == --verify ]]; then verify "${2:-}"; exit $?; fi
commands=(
'cargo fmt -p mir-client -- --check'
'cargo clippy -p mir-client --all-targets -- -D warnings'
'cargo test -p mir-client --lib r64_'
'cargo test -p mir-client --test round64_event_reclaim'
'cargo test -p mir-client --test round64_projectile_ack'
'cargo test -p mir-client --bin mir-client r64_'
'cargo test -p mir-client --test round59_projectile_render'
'cargo test -p mir-client --test round61_projectile_queue'
'cargo test -p mir-client --test round62_projectile_wire'
'cargo test -p mir-client --test round63_projectile_wire'
'py -3 tools/docs/check_links.py'
'git diff --check'
'bash docs/testing/round64/gen-tree-hashes.sh | diff -u docs/testing/round64/commit-tree-hashes.txt -'
'bash docs/testing/round64/run-checks.sh --verify gate'
'bash docs/testing/round64/run-checks.sh --verify meta'
'bash docs/testing/round64/run-checks.sh --verify manifest'
'bash docs/testing/round64/run-checks.sh --verify claims'
'bash docs/testing/round64/run-checks.sh --verify attributes'
'bash docs/testing/round64/run-checks.sh --verify negative'
'bash -n docs/testing/round64/run-checks.sh && bash -n docs/testing/round64/gen-tree-hashes.sh'
)
if [[ ${1:-} == --list ]]; then printf '%s\n' "${commands[@]}"; exit 0; fi
[[ $# == 0 ]] || { echo 'Usage: run-checks.sh [--list|--verify mode]'; exit 2; }
log_dir=${R64_LOG_DIR:-target/round64/d/attempt-1}
mkdir -p "$log_dir/tmp" || exit 2
if command -v cygpath >/dev/null 2>&1; then export TEMP="$(cygpath -m "$PWD/$log_dir/tmp")" TMP="$(cygpath -m "$PWD/$log_dir/tmp")"; else export TEMP="$PWD/$log_dir/tmp" TMP="$PWD/$log_dir/tmp"; fi
pass=0; fail=0; skip=0; total_start=$SECONDS
: > "$log_dir/summary.tsv"
for i in "${!commands[@]}"; do
  cmd=${commands[$i]}; number=$((i+1)); start=$SECONDS; log="$log_dir/$number.log"
  printf 'RUN %s UTC=%s command=%s\n' "$number" "$(date -u +%FT%TZ)" "$cmd" | tee "$log_dir/$number.meta.txt"
  bash -o pipefail -c "$cmd" > "$log" 2>&1; rc=$?
  status=PASS
  if [[ $rc != 0 ]]; then status=FAIL
  elif [[ $cmd == 'cargo test '* ]] && ! grep -Eq 'test result: ok\. [1-9][0-9]* passed; 0 failed; 0 ignored;' "$log"; then status=FAIL
  elif grep -Eq '(^|[[:space:]])SKIP(:|[[:space:]])' "$log"; then status=FAIL; fi
  if [[ $status == PASS ]]; then pass=$((pass+1)); else fail=$((fail+1)); fi
  printf '%s exit=%s seconds=%s utc_end=%s command=%s\n' "$status" "$rc" "$((SECONDS-start))" "$(date -u +%FT%TZ)" "$cmd" | tee -a "$log_dir/summary.tsv" "$log_dir/$number.meta.txt"
  if [[ $status != PASS ]]; then tail -20 "$log"; fi
done
printf 'SUMMARY PASS=%s FAIL=%s SKIP=%s TOTAL=%s seconds=%s\n' "$pass" "$fail" "$skip" "${#commands[@]}" "$((SECONDS-total_start))" | tee -a "$log_dir/summary.tsv"
[[ $fail == 0 && $skip == 0 ]]
