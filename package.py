"""ZIP creation requires an explicit final release."""
from pathlib import Path
import argparse,shutil,json,zipfile
from privacy_audit import audit
root=Path(__file__).resolve().parent
version='0.33'
parser=argparse.ArgumentParser()
parser.add_argument('--out',type=Path,default=root/'release',help='Output directory (default: repository/release)')
parser.add_argument('--final',action='store_true',help='Create source ZIP only after user declares final version')
args=parser.parse_args()
audit(root/'dist')
out=args.out.resolve();dest=out/f'OfficeTabs-Rust-{version}';dest.mkdir(parents=True,exist_ok=True)
allowed=['Setup.exe','OfficeTabs.Manager.exe','OfficeTabs.dll','uninstaller.exe','OfficeTabs.msi','OfficeTabs.ico','LICENSE.txt','NOTICE.txt','README.txt','privacy-audit.json','native-audit.json','msi-audit.json']
for name in allowed:shutil.copy2(root/'dist'/name,dest/name)
setup=out/f'OfficeTabs-Rust-{version}-Setup.exe';shutil.copy2(root/'dist/Setup.exe',setup)
result={'installer':setup.name,'installer_bytes':setup.stat().st_size,'zip_created':False}
if args.final:
 archive=out/f'OfficeTabs-Rust-{version}-Developer-Backup.zip';files=[]
 for p in root.rglob('*'):
  rel=p.relative_to(root)
  if not p.is_file() or any(part in ('target','msi','dist','release','.git','__pycache__') for part in rel.parts) or p.is_relative_to(out):continue
  if p.suffix in ('.pdb','.lib','.exp','.pyc','.log'):continue
  files.append(p)
 audit(root,files)
 with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED) as z:
  for p in files:z.write(p,'OfficeTabs-Rust/'+p.relative_to(root).as_posix())
 with zipfile.ZipFile(archive) as z:assert z.testzip() is None
 result.update(zip_created=True,archive=archive.name)
print(json.dumps(result))
