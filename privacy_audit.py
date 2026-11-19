"""Release identity and machine-path check, not an anonymity guarantee."""
from pathlib import Path
import os,json,re,argparse
def audit(root,files=None):
 root=Path(root)
 if files is None:
  names=['Setup.exe','OfficeTabs.Manager.exe','OfficeTabs.dll','uninstaller.exe','OfficeTabs.msi','OfficeTabs.ico','LICENSE.txt','NOTICE.txt','README.txt']
  files=[root/name for name in names]
 user=os.environ.get('USERNAME','');profile=os.environ.get('USERPROFILE','')
 tokens=[s for s in (user,profile) if len(s)>3]
 if user:tokens.append('n'+user)
 failures=[]
 for p in files:
  data=p.read_bytes()
  for encoding in ('utf-8','utf-16-le','utf-16-be'):
   text=data.decode(encoding,errors='ignore').casefold()
   if any(t.casefold() in text for t in tokens) or re.search(r'[a-z]:[\\/]users[\\/]',text) or re.search(r'\bs-1-5-21-\d+',text):
    failures.append(p.relative_to(root).as_posix());break
 if failures:raise SystemExit('Privacy scan failed in: '+', '.join(failures))
 if root.name=='dist':
  (root/'privacy-audit.json').write_text(json.dumps({'personal_identity_matches':0,'local_user_path_matches':0,'checked_files':[p.name for p in files],'publisher':'OfficeTabs','scope':'Release payload only; historical files, installed copies, timestamps and third-party licenses are not erased.'},indent=2),encoding='utf-8')
 return True
if __name__=='__main__':
 parser=argparse.ArgumentParser()
 parser.add_argument('--source',action='store_true')
 args=parser.parse_args();root=Path(__file__).resolve().parent
 if args.source:
  files=[p for p in root.rglob('*') if p.is_file() and not set(p.relative_to(root).parts).intersection({'target','msi','dist','release','.git','__pycache__'})]
  audit(root,files)
  print('Source privacy checks passed.')
 else:
  audit(root/'dist')
  print('Release privacy checks passed.')
