"""Read-only audit of evidence batches, PNG hashes and historical changes."""
from pathlib import Path
from collections import Counter, defaultdict
from datetime import datetime, timezone
import hashlib, json, subprocess, struct, zlib
ROOT=Path(__file__).resolve().parents[3]
OUT=Path(__file__).with_name('audit-detail-results.json')
def git(*args):
    return subprocess.run(['git',*args],cwd=ROOT,capture_output=True,check=True).stdout
def load(p):
    return json.loads((ROOT/p).read_text(encoding='utf-8-sig'))
def digest(b): return hashlib.sha256(b).hexdigest()
def sha(p): return digest((ROOT/p).read_bytes())
def decode_png(b):
    at=8; data=bytearray(); ihdr=None
    while at<len(b):
        n=struct.unpack('>I',b[at:at+4])[0]; kind=b[at+4:at+8]; val=b[at+8:at+8+n]
        if kind==b'IHDR': ihdr=struct.unpack('>IIBBBBB',val)
        if kind==b'IDAT': data.extend(val)
        at+=n+12
        if kind==b'IEND': break
    w,h,depth,color,_,_,inter=ihdr
    assert depth==8 and color in (2,6) and inter==0
    ch=4 if color==6 else 3; stride=w*ch; raw=zlib.decompress(data); prev=bytearray(stride); pix=bytearray(); p=0
    for _ in range(h):
        f=raw[p]; row=bytearray(raw[p+1:p+1+stride]); p+=stride+1
        for x in range(stride):
            a=row[x-ch] if x>=ch else 0; u=prev[x]; c=prev[x-ch] if x>=ch else 0
            if f==0: v=0
            elif f==1: v=a
            elif f==2: v=u
            elif f==3: v=(a+u)//2
            elif f==4:
                q=a+u-c; ds=(abs(q-a),abs(q-u),abs(q-c)); v=(a,u,c)[ds.index(min(ds))]
            else: raise ValueError('filter')
            row[x]=(row[x]+v)&255
        pix.extend(row); prev=row
    if ch==3: pix=bytearray(c for i in range(0,len(pix),3) for c in (*pix[i:i+3],255))
    return w,h,pix
def fnv(b):
    n=0xcbf29ce484222325
    for x in b: n=((n^x)*0x100000001b3)&0xffffffffffffffff
    return f'{n:016x}'
def crop(pix,w,h):
    xs=[]; ys=[]
    for i in range(3,len(pix),4):
        if pix[i]: xs.append(i//4%w); ys.append(i//4//w)
    if not xs: return None,b''
    x0=min(xs); x1=max(xs)+1; y0=min(ys); y1=max(ys)+1
    return [x0,y0,x1-x0,y1-y0],b''.join(pix[(y*w+x0)*4:(y*w+x1)*4] for y in range(y0,y1))
r={'sampled_at':datetime.now(timezone.utc).isoformat(),'head':git('rev-parse','HEAD').decode().strip()}
base=load('docs/testing/round49/audit-probe-results.json')
paths=sorted(set(p for p in (git('diff','--name-only','-z')+git('ls-files','--others','--exclude-standard','-z')).decode().split('\0') if p))
r['inventory']=[{'path':p,'bytes':(ROOT/p).stat().st_size,'sha256':sha(p)} for p in paths if (ROOT/p).is_file()]
r['changed_since_first_probe']=[{'path':f['path'],'before':f['sha256'],'after':sha(f['path'])} for f in base['files'] if (ROOT/f['path']).is_file() and f['sha256']!=sha(f['path'])]
flags=base['frame_flags']['flagged_actions']
r['flags']={'reverse_by_action':dict(Counter(str(a['action']) for a in flags if a['reverse'])),'blend':[a for a in flags if a['blend']],'effects':[a for a in flags if a['effect_count']]}
before=load('docs/testing/monster-native/player-resource-freeze-before.json'); after=load('docs/testing/monster-native/player-resource-freeze.json')
r['player_freeze']={'before_counts':before.get('counts'),'after_counts':after.get('counts'),'before_keys':list(before),'after_keys':list(after)}
for k in before:
    if k in after and isinstance(before[k],dict): r['player_freeze'][k+'_equal']=before[k]==after[k]
old=json.loads(git('show','HEAD:docs/testing/attack-variants/live-manifest.json')); new=load('docs/testing/attack-variants/live-manifest.json')
r['attack']={'old_batch':old.get('batch_id'),'new_batch':new.get('batch_id'),'old_packs':old.get('release_pack_check'),'new_packs':new.get('release_pack_check'),'samples':[]}
for a,b in zip(old['samples'],new['samples']):
    name=Path(b['png'].replace('\\','/')).name; p='docs/testing/attack-variants/'+name; data=(ROOT/p).read_bytes(); w,h,pix=decode_png(data)
    fields=['action','frame_index','actual_index','attack_frame_index','logical_index','layers','action_now','variant','view_variant','elapsed_ms','camera_center','self_screen']
    changes={k:{'before':a.get(k),'after':b.get(k)} for k in fields if a.get(k)!=b.get(k)}
    r['attack']['samples'].append({'png':p,'png_byte_hash_matches':fnv(data)==b.get('png_byte_hash'),'pixel_hash_matches':fnv(pix)==b.get('pixel_hash'),'changes':changes,'old_pixel_hash':a.get('pixel_hash'),'new_pixel_hash':b.get('pixel_hash')})
m=load('docs/testing/monster-native/manifest.json'); checks=[]
for item in m.get('png_files',[]):
    p=item.get('path',item.get('png')); data=(ROOT/p).read_bytes(); w,h,pix=decode_png(data); bbox,sub=crop(pix,w,h)
    checks.append({'png':p,'png_sha_matches':digest(data)==item.get('png_sha256'),'crop_sha_matches':digest(sub)==item.get('crop_sha256'),'bbox_actual':bbox,'bbox_manifest':item.get('crop_bbox')})
r['native_manifest_png_checks']=checks
r['native_metadata']={'manifest_batch':m.get('batch'),'raw_generated':load('docs/testing/monster-native/raw-manifest.json').get('generated_at_unix_ms'),'frozen_batch':load('docs/testing/monster-native/freeze.json').get('batch'),'native_packs':len(m.get('assets',{}).get('native_packs',[])),'current_source_sha256':{p:sha(p) for p in m.get('baseline',{}).get('source_blobs',{})},'manifest_source_sha256':m.get('baseline',{}).get('source_blobs')}
for rel in ['target/gate-r49-final2.log','target/r49-evidence-tests.log']:
    data=(ROOT/rel).read_bytes(); text=data.decode('utf-8',errors='replace'); lines=text.splitlines()
    r[rel]={'sha256':digest(data),'utf8_replacements':text.count('\ufffd'),'tail':[{'line':i+1,'text':s} for i,s in enumerate(lines) if i>=len(lines)-60]}
r['duplicates']={}
for prefix in ['docs/testing/monster-native/','docs/testing/monster-motion/']:
    groups=defaultdict(list)
    for f in r['inventory']:
        if f['path'].startswith(prefix) and f['path'].endswith('.png'): groups[f['sha256']].append(f['path'])
    r['duplicates'][prefix]=[v for v in groups.values() if len(v)>1]
OUT.write_text(json.dumps(r,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'output':str(OUT),'changed_since_first_probe':r['changed_since_first_probe'],'flags':r['flags'],'attack_samples':len(r['attack']['samples']),'attack_hash_failures':[a for a in r['attack']['samples'] if not a['png_byte_hash_matches'] or not a['pixel_hash_matches']],'attack_semantic_changes':[a for a in r['attack']['samples'] if a['changes']],'native_png_checks':len(checks),'native_png_failures':[a for a in checks if not a['png_sha_matches'] or not a['crop_sha_matches']],'native_metadata':r['native_metadata'],'player_freeze':r['player_freeze']},ensure_ascii=False,indent=2))
