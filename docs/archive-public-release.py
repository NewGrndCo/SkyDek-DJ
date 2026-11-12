from pathlib import Path
import hashlib, subprocess, zipfile

root = Path(__file__).resolve().parents[1] / 'release' / 'SkyDek-DJ-0.6.0-Windows'
archive = root.parent / (root.name + '.zip')
files = subprocess.check_output(['git', 'ls-files', '-z'], cwd=root).decode().strip('\0').split('\0')
assert len(files) == 7, 'Review unexpected distribution files before publishing'
assert not any('.git/' in f or f.startswith('src/') for f in files)
with zipfile.ZipFile(archive, 'x', zipfile.ZIP_DEFLATED) as z:
    for name in files:
        z.write(root/name, root.name+'/'+name)
with zipfile.ZipFile(archive) as z:
    assert z.testzip() is None
    installer = z.read(root.name+'/SkyDek-DJ-0.6.0-Windows-Setup.exe')
    assert hashlib.sha256(installer).hexdigest() == '23069c850d3250173ca34b529ab7602484b84de23d3834e66da3761c39b4a634'
print(str(archive))
print(archive.stat().st_size)
