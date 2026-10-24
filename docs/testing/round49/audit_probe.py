"""Read-only R49 audit probe; writes only its result beside this script."""
from pathlib import Path
import hashlib
import json
import struct
import subprocess
import zlib

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).with_name("audit-probe-results.json")

def sha(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for b in iter(lambda: f.read(1024 * 1024), b""):
            h.update(b)
    return h.hexdigest()

def git(*args):
    p = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, check=True)
    return p.stdout

def load(path):
    return json.loads((ROOT / path).read_text(encoding="utf-8-sig"))

def png(path):
    b = path.read_bytes()
    assert b[:8] == b"\x89PNG\r\n\x1a\n", "signature"
    at, chunks, data, ihdr = 8, [], bytearray(), None
    while at < len(b):
        n = struct.unpack(">I", b[at:at+4])[0]
        kind = b[at+4:at+8]
        value = b[at+8:at+8+n]
        crc = struct.unpack(">I", b[at+8+n:at+12+n])[0]
        assert zlib.crc32(kind+value) & 0xffffffff == crc, "CRC"
        if kind == b"IHDR":
            ihdr = struct.unpack(">IIBBBBB", value)
        if kind == b"IDAT":
            data.extend(value)
        chunks.append(kind.decode("ascii"))
        at += n + 12
        if kind == b"IEND":
            break
    assert at == len(b) and chunks[-1] == "IEND", "IEND/trailing"
    w, h, depth, color, comp, filt, inter = ihdr
    assert depth == 8 and color in (2,6) and inter == 0, repr(ihdr)
    channels = 4 if color == 6 else 3
    scan = zlib.decompress(data)
    stride = w * channels
    assert len(scan) == h * (stride+1), "length"
    pixels, prev, p = bytearray(), bytearray(stride), 0
    for y in range(h):
        f = scan[p]
        row = bytearray(scan[p+1:p+1+stride])
        p += stride+1
        for x in range(stride):
            left = row[x-channels] if x >= channels else 0
            up = prev[x]
            ul = prev[x-channels] if x >= channels else 0
            if f == 0:
                v = 0
            elif f == 1:
                v = left
            elif f == 2:
                v = up
            elif f == 3:
                v = (left+up)//2
            elif f == 4:
                q = left+up-ul
                ds = (abs(q-left), abs(q-up), abs(q-ul))
                v = (left,up,ul)[ds.index(min(ds))]
            else:
                raise AssertionError("filter")
            row[x] = (row[x]+v)&255
        pixels.extend(row)
        prev = row
    if channels == 3:
        pixels = bytearray(c for i in range(0,len(pixels),3) for c in (*pixels[i:i+3],255))
    return {"width":w,"height":h,"png_sha256":sha(path),"pixels_sha256":hashlib.sha256(pixels).hexdigest()}

status = git("status", "--porcelain").decode("utf-8")
tracked = git("diff", "--name-only", "-z").decode("utf-8").split("\0")
untracked = git("ls-files", "--others", "--exclude-standard", "-z").decode("utf-8").split("\0")
paths = sorted(set(p for p in tracked+untracked if p))
result = {"head":git("rev-parse","HEAD").decode().strip(), "status":status.splitlines(), "files":[],"json":{},"png_errors":[],"hash_checks":[],"frame_flags":{},"errors":[]}
for rel in paths:
    p = ROOT/rel
    if not p.is_file():
        result["errors"].append({"path":rel,"error":"not regular file"})
        continue
    item={"path":rel,"bytes":p.stat().st_size,"sha256":sha(p),"mtime_ns":p.stat().st_mtime_ns,"untracked":rel in untracked}
    if p.suffix.lower()==".png":
        try:
            item.update(png(p))
        except Exception as ex:
            result["png_errors"].append({"path":rel,"error":str(ex)})
    else:
        try:
            text=p.read_text(encoding="utf-8-sig")
            item["utf8"]=True
            if p.suffix==".json":
                j=json.loads(text)
                result["json"][rel]={"keys":list(j) if isinstance(j,dict) else [],"batch":j.get("batch",j.get("batch_id")) if isinstance(j,dict) else None,"generated":j.get("generated_at_unix_ms") if isinstance(j,dict) else None}
        except Exception as ex:
            item["utf8"]=False
            result["errors"].append({"path":rel,"error":str(ex)})
    result["files"].append(item)

def walk(obj, loc, base):
    if isinstance(obj,dict):
        for key,val in obj.items():
            if isinstance(val,str) and len(val)==64 and all(c in "0123456789abcdef" for c in val):
                stem = key.removesuffix("_sha256").removesuffix("_hash")
                candidates = [obj.get(stem),obj.get("path") if key in ("sha256","png_sha256","pixels_sha256") else None,obj.get("png") if key=="png_sha256" else None]
                for s in candidates:
                    if not isinstance(s,str):
                        continue
                    path=ROOT/s.replace("\\","/")
                    if not path.is_file():
                        alt=base/s
                        if alt.is_file():
                            path=alt
                    if path.is_file() and key!="pixels_sha256":
                        actual=sha(path)
                        result["hash_checks"].append({"at":loc+"/"+key,"path":str(path.relative_to(ROOT)),"expected":val,"actual":actual,"match":actual==val})
                        break
            walk(val,loc+"/"+key,base)
    elif isinstance(obj,list):
        for i,val in enumerate(obj):
            walk(val,loc+"/"+str(i),base)

for rel in ["docs/testing/monster-native/manifest.json","docs/testing/monster-native/freeze.json","docs/testing/monster-native/raw-manifest.json","docs/testing/monster-motion/manifest.json","docs/testing/monster-motion/stride-zero-render.json","docs/testing/attack-variants/live-manifest.json"]:
    if (ROOT/rel).is_file():
        walk(load(rel),rel,(ROOT/rel).parent)
frames_root=ROOT/"game-assets/gfx/sprites/monster-native"
flagged=[]
for p in sorted(frames_root.glob("*/frames.json")):
    j=json.loads(p.read_text(encoding="utf-8-sig"))
    for a in j.get("actions",[]):
        if a.get("reverse") or a.get("blend") or a.get("effect_count",0)>0:
            flagged.append({"key":p.parent.name,"action":a.get("raw_action"),"reverse":a.get("reverse"),"blend":a.get("blend"),"effect_count":a.get("effect_count")})
result["frame_flags"]={"libraries":len(list(frames_root.glob("*/frames.json"))),"flagged_actions":flagged}
result["over_1MB"]=[f for f in result["files"] if f["bytes"]>1_000_000]
result["hash_mismatches"]=[f for f in result["hash_checks"] if not f["match"]]
result["directory_bytes"]={str(p.relative_to(ROOT)):sum(f.stat().st_size for f in p.rglob("*") if f.is_file()) for p in [ROOT/"docs/testing/monster-native",ROOT/"docs/testing/monster-motion",ROOT/"docs/testing/native-monster"]}
OUT.write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
print(json.dumps({"files":len(result["files"]),"bytes":sum(f["bytes"] for f in result["files"]),"over_1MB":[{"path":f["path"],"bytes":f["bytes"]} for f in result["over_1MB"]],"png_errors":result["png_errors"],"text_errors":result["errors"],"hash_checks":len(result["hash_checks"]),"hash_mismatches":result["hash_mismatches"],"directory_bytes":result["directory_bytes"],"flagged_actions":len(flagged),"output":str(OUT)},ensure_ascii=False,indent=2))
