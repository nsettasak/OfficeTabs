"""Release identity and machine-path check, not an anonymity guarantee."""
from pathlib import Path
import os,json,re,argparse
def audit(root,files=None):
 root=Path(root)
 if files is None:
  names=['Setup.exe','OfficeTabs.Manager.exe','OfficeTabs.dll','uninstaller.exe','OfficeTabs.msi','OfficeTabs.cab','OfficeTabs.ico','LICENSE.txt']
  files=[root/name for name in names if (root/name).is_file()]
 user=os.environ.get('USERNAME','');profile=os.environ.get('USERPROFILE','')
 tokens=[s for s in (user,profile) if len(s)>3]
 # The public repository handle is deliberately published; strip it before the
 # username check so README/Cargo.toml links do not count as a leak. Local paths
 # (C:\\Users\\...) are still caught by the pattern below.
 public=[s.casefold() for s in ('nsettasak','github.com/nsettasak')]
 failures=[]
 for p in files:
  data=p.read_bytes()
  for encoding in ('utf-8','utf-16-le','utf-16-be'):
   text=data.decode(encoding,errors='ignore').casefold()
   for s in public:text=text.replace(s,'')
   if any(t.casefold() in text for t in tokens) or re.search(r'[a-z]:[\\/]users[\\/]',text) or re.search(r'\bs-1-5-21-\d+',text):
    failures.append(p.relative_to(root).as_posix());break
 if failures:raise SystemExit('Privacy scan failed in: '+', '.join(failures))
 return True
if __name__=='__main__':
 parser=argparse.ArgumentParser()
 parser.add_argument('--source',action='store_true',help='scan the repository instead of a built payload')
 parser.add_argument('--payload',type=Path,help='staging folder holding the built payload')
 args=parser.parse_args()
 root=Path(__file__).resolve().parents[1]  # repository root; this file lives in scripts/
 if args.source:
  files=[p for p in root.rglob('*') if p.is_file() and not set(p.relative_to(root).parts).intersection({'target','.git','__pycache__'})]
  audit(root,files)
  print('Source privacy checks passed.')
 elif args.payload:
  audit(args.payload)
  print('Payload privacy checks passed.')
 else:
  parser.error('choose --source or --payload <folder>')
