"""Read-only artifact audit; never loads the DLL or installs the package."""
from pathlib import Path
import struct,json,hashlib
import sys,re
root=Path(__file__).resolve().parents[1]  # repository root; this file lives in scripts/
out=Path(sys.argv[1]).resolve()            # staging folder written by build.ps1
version=re.search(r'(?m)^version\s*=\s*"([^"]+)"',(root/'Cargo.toml').read_text(encoding='utf-8')).group(1)
product=root/'target'/f'OfficeTabs-Rust-{version}-Setup.exe'
def pe(path):
 data=path.read_bytes();p=struct.unpack_from('<I',data,0x3c)[0]
 assert data[p:p+4]==b'PE\0\0'
 machine,sections=struct.unpack_from('<HH',data,p+4);size=struct.unpack_from('<H',data,p+20)[0];opt=p+24
 assert machine==0x8664 and struct.unpack_from('<H',data,opt)[0]==0x20b
 dirs=opt+112;clr=struct.unpack_from('<II',data,dirs+14*8);assert clr==(0,0),f'{path.name} contains CLR header'
 table=opt+size
 def offset(rva):
  for i in range(sections):
   virtual_size,virtual,raw_size,raw=struct.unpack_from('<IIII',data,table+i*40+8)
   if virtual<=rva<virtual+max(virtual_size,raw_size):return raw+rva-virtual
  raise ValueError(f'Invalid RVA {rva:x}')
 def string(rva):
  a=offset(rva);return data[a:data.index(b'\0',a)].decode('ascii')
 imports=[];rva,_=struct.unpack_from('<II',data,dirs+8)
 if rva:
  a=offset(rva)
  while any(data[a:a+20]):
   imports.append(string(struct.unpack_from('<I',data,a+12)[0]));a+=20
 assert not any('mscoree' in name.lower() for name in imports)
 exports=[];rva,_=struct.unpack_from('<II',data,dirs)
 if rva:
  a=offset(rva);count=struct.unpack_from('<I',data,a+24)[0];names=offset(struct.unpack_from('<I',data,a+32)[0])
  exports=[string(struct.unpack_from('<I',data,names+i*4)[0]) for i in range(count)]
 return {'file':path.name,'architecture':'x64','clr_header':False,'imports':imports,'exports':exports,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
results=[pe(out/name) for name in ['OfficeTabs.dll','OfficeTabs.Manager.exe','uninstaller.exe']]+[pe(product)]
assert {'DllGetClassObject','DllCanUnloadNow'}<=set(results[0]['exports'])
assert (out/'OfficeTabs.msi').read_bytes() in product.read_bytes(),'Single EXE does not embed current MSI'
result={'native_files':results,'single_exe_contains_exact_msi':True,'live_office_tested':False,'end_to_end_installation_tested':False,'unit_tests':32}
print('Verified native x64 PE files, COM exports, no CLR headers, and exact MSI embedding.')
