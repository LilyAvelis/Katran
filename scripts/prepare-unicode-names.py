from pathlib import Path
import urllib.request,json,hashlib
root=Path('assets/unicode')
urls={'emoji-test.txt':'https://www.unicode.org/Public/emoji/16.0/emoji-test.txt','UnicodeData.txt':'https://www.unicode.org/Public/16.0.0/ucd/UnicodeData.txt','LICENSE.txt':'https://www.unicode.org/license.txt'}
data={k:urllib.request.urlopen(u,timeout=30).read() for k,u in urls.items()}
emoji={}
for line in data['emoji-test.txt'].decode().splitlines():
 if not line or line.startswith('#') or ';' not in line:continue
 left,comment=line.split('#',1); cps,status=left.split(';',1)
 if status.strip() not in ('fully-qualified','minimally-qualified','unqualified'):continue
 value=''.join(chr(int(cp,16)) for cp in cps.split())
 tail=comment.strip().split(' ',2)
 if len(tail)==3:emoji[value]=tail[2]
names={}
for line in data['UnicodeData.txt'].decode().splitlines():
 parts=line.split(';'); cp=int(parts[0],16)
 if parts[2].startswith('S') or cp in (0x200d,0xfe0e,0xfe0f,0x20e3,0x20,0x23,0x2a) or 0x30<=cp<=0x39 or 0xe0020<=cp<=0xe007f:names[parts[0]]=parts[1]
(root/'names.json').write_text(json.dumps({'emoji':emoji,'codepoints':names},ensure_ascii=False,separators=(',',':')),encoding='utf8')
(root/'LICENSE.txt').write_bytes(data['LICENSE.txt'])
(root/'SOURCE.md').write_text('Unicode 16.0 names, generated from the official files.\n'+''.join(f'{urls[k]} SHA256 {hashlib.sha256(v).hexdigest()}\n' for k,v in data.items()),encoding='utf8')
print(len(emoji),len(names))
