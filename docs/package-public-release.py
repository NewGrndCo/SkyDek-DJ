"""Assemble a binary-only public distribution from an existing verified installer."""
from pathlib import Path
import hashlib, json, os, shutil, struct, zipfile
import urllib.request

root = Path(__file__).resolve().parents[1]
version = json.loads((root / 'package.json').read_text())['version']
out = root / 'release' / f'SkyDek-DJ-{version}-Windows'
out.mkdir(parents=True, exist_ok=True)
installer = root / f'src-tauri/target/release/bundle/nsis/SkyDek DJ_{version}_x64-setup.exe'
public_name = f'SkyDek-DJ-{version}-Windows-Setup.exe'
shutil.copy2(installer, out / public_name)
digest = hashlib.sha256(installer.read_bytes()).hexdigest()
(out / 'assets').mkdir(exist_ok=True)
shutil.copy2(root / 'public/brand.png', out / 'assets/brand.png')
notices = out / 'THIRD-PARTY'
notices.mkdir(exist_ok=True)
metadata = json.loads((Path(os.environ['TEMP']) / 'skydek-release-cargo-metadata.json').read_text(encoding='utf-8-sig'))
nodes = {n['id'] for n in metadata['resolve']['nodes']}
packages = [p for p in metadata['packages'] if p['id'] in nodes and p['source']]
records, missing = [], []
license_cache = root / 'docs' / 'dependency-license-cache'
license_cache.mkdir(exist_ok=True)
mpl = []
for p in packages:
    folder = Path(p['manifest_path']).parent
    files = [f for f in folder.iterdir() if f.is_file() and f.name.lower().startswith(('license', 'licence', 'copying', 'notice'))]
    if p.get('license_file'):
        f = folder / p['license_file']
        if f.exists() and f not in files: files.append(f)
    if not files:
        vcs_file = folder / '.cargo_vcs_info.json'
        vcs = json.loads(vcs_file.read_text()) if vcs_file.exists() else {}
        commit = vcs.get('git', {}).get('sha1')
        repo = (p.get('repository') or '').removesuffix('.git').replace('https://github.com/', '')
        if commit and repo and '/' in repo:
            cache_name = repo.replace('/', '-')+'-'+commit
            cached = list(license_cache.glob(cache_name+'-*'))
            if not cached:
                for candidate in ['LICENSE', 'LICENSE-MIT', 'LICENSE.txt', 'COPYING']:
                    url = f'https://raw.githubusercontent.com/{repo}/{commit}/{candidate}'
                    try:
                        payload = urllib.request.urlopen(url, timeout=12).read()
                        dest = license_cache / (cache_name+'-'+candidate)
                        dest.write_bytes(payload)
                        cached = [dest]
                        break
                    except Exception:
                        pass
            files.extend(cached)
        if not files and p.get('license') == 'MPL-2.0':
            common = license_cache / 'MPL-2.0.txt'
            if not common.exists():
                common.write_bytes(urllib.request.urlopen('https://www.mozilla.org/media/MPL/2.0/index.txt', timeout=15).read())
            files.append(common)
    records.append(f"\n{'='*72}\n{p['name']} {p['version']}\nLicense: {p.get('license') or 'See license text'}\nSource: {p.get('repository') or 'https://crates.io/crates/'+p['name']+'/'+p['version']}\n")
    if not files: missing.append(p['name'])
    for f in files: records.append(f'\n--- {f.name} ---\n'+f.read_text(encoding='utf-8', errors='replace'))
    if 'MPL' in (p.get('license') or ''): mpl.append((p, folder))
lock = json.loads((root / 'package-lock.json').read_text())
for name, p in lock['packages'].items():
    if not name or p.get('dev'): continue
    folder = root / name
    if not folder.exists(): continue
    files = [f for f in folder.iterdir() if f.is_file() and f.name.lower().startswith(('license', 'licence', 'copying', 'notice'))]
    pkg = json.loads((folder/'package.json').read_text())
    records.append(f"\n{'='*72}\n{pkg['name']} {pkg['version']}\nLicense: {pkg.get('license', p.get('license', 'See license text'))}\nSource: https://www.npmjs.com/package/{pkg['name']}/v/{pkg['version']}\n")
    if not files: missing.append(pkg['name'])
    for f in files: records.append(f'\n--- {f.name} ---\n'+f.read_text(encoding='utf-8', errors='replace'))
(notices/'NOTICES.txt').write_text('SkyDek DJ — third-party dependency notices\n\nThis inventory includes resolved Windows dependencies and build components; inclusion does not imply every component ships at runtime. Each third-party component retains its own license. These licenses do not grant rights to proprietary SkyDek application code or branding.\n'+''.join(records), encoding='utf-8')
with zipfile.ZipFile(notices/'MPL-DEPENDENCY-SOURCES.zip', 'w', zipfile.ZIP_DEFLATED, strict_timestamps=False) as z:
    if (license_cache/'MPL-2.0.txt').exists(): z.write(license_cache/'MPL-2.0.txt', 'MPL-2.0.txt')
    for p, folder in mpl:
        for f in folder.rglob('*'):
            if f.is_file(): z.write(f, f"{p['name']}-{p['version']}/"+f.relative_to(folder).as_posix())
(notices/'README.md').write_text('# Third-party components\n\nNOTICES.txt contains upstream license and attribution texts for resolved dependencies. MPL-DEPENDENCY-SOURCES.zip contains the unmodified MPL-licensed dependency source used by this build. It contains no proprietary SkyDek application source. Microsoft WebView2 is supplied by Microsoft under its own terms; the setup program downloads it only when needed.\n', encoding='utf-8')
(out/'.gitignore').write_text('.DS_Store\nThumbs.db\nDesktop.ini\n', encoding='utf-8')
readme = '''<p align="center"><img src="assets/brand.png" width="88" alt="SkyDek logo"></p>

# SkyDek DJ

**Your music. Your controller. Your mix.**

SkyDek DJ is a standalone Windows DJ application by **New Ground Enterprise**, designed for mixing local music with an Xbox controller or standard mouse controls. Browse tracks, control two decks, trigger sound effects, and record your mix from one dark, focused workspace.

**Version 0.6.0 · Windows x64 · Preview release**

## Download and install

1. [Download the Windows setup file](./INSTALLER_NAME). On GitHub's file page, use **Download raw file** if a download does not start.
2. If you downloaded this repository as a ZIP, extract it first.
3. Exit an older SkyDek instance using **Exit** in its system-tray menu. Closing its window alone leaves it running.
4. Run **INSTALLER_NAME** and follow the installer.
5. Open **SkyDek DJ** from the Start menu or your installed shortcut.

This repository distributes the ready-to-install application, not its proprietary source code. No terminal commands or development tools are needed.

### Requirements and dependencies

- A 64-bit Windows PC. Windows 11 is recommended; this is not a macOS, Linux, or Windows ARM64 build.
- Microsoft Edge **WebView2 Runtime**. Setup detects it and downloads/installs it if absent, so allow an internet connection during first installation. A full offline runtime is not included. [Microsoft's official WebView2 download](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).
- A working Windows audio output. External audio interfaces need their manufacturer's Windows drivers.
- Optional: an Xbox Series or other XInput-compatible controller, paired in Windows over Bluetooth or connected with a data-capable USB cable. Mouse controls work without a controller.
- Your own WAV or MP3 music and sound effects. No music or sample library is included.

**loopMIDI, VirtualDJ, a virtual MIDI driver, Node.js, Rust, and Visual Studio are not required.** This standalone application controls its own audio; it does not use the older SkyDek Mapper-to-VirtualDJ setup. All application components are included in setup, except Microsoft's automatically downloaded runtime and any hardware-specific drivers.

## Make your first mix

1. Choose **Import folder** in Browser, select your music folder, and navigate its subfolders on the left. The adjacent library shows tracks in the selected folder.
2. Use a track's **1** or **2** button to load a deck. Press its Play button to begin. Use **CUE** to return to the cue mark; click a waveform to seek and **SET CUE** to place the cue there.
3. Load a second track into the other deck. Adjust deck levels and the crossfader to transition between them.
4. Hold a deck effect button while you want the effect active; release to disengage it. Active effects highlight.
5. **RECORD WAV** chooses a destination for recording the master mix. Stop recording to finalize the WAV file before exiting.

Playing decks are protected from accidental controller loads. Choose the deck with the mouse and confirm if you intentionally want to replace a playing track.

## Xbox controls — default Normal layer

| Control | Action |
|---|---|
| A | Load highlighted track into a preparation deck |
| B | Cue preview; hold for the configured time, then release to keep playing |
| X | Load the next Sidelist track |
| Y | Add highlighted track to Sidelist |
| D-pad left / right | Move focus between Folders, Library, and Sampler |
| D-pad up / down | Browse the focused panel |
| Left stick | Slide the crossfader; release to keep its position |
| L3 — click left stick | Automatic crossfade to the opposite side |
| Right stick horizontal / vertical | Target deck filter / effect mix |
| R3 — click right stick | Trigger the highlighted sampler pad |
| LB / RB | Hold backspin / brake effect |
| Menu | Change target deck |
| View — overlapping squares | Toggle Normal / Shift layer |

Shift provides alternate assignments, including explicit deck loading, echo, reverb, tempo matching, and next-sample selection. Check **Settings → SkyDek Mapper** to see or change either layer, save mappings, and manage named profiles. Existing users may have different saved assignments. The Xbox **Share** button is reserved by Windows and is not used as the Shift button.

After clicking L3/R3 or reconnecting, return that stick to center to re-enable its analog movement. Battery level appears only when reported by Windows and is approximate.

## Add and trigger sound effects

1. In **SAMPLER**, select a pad and open **Sampler settings / Add sounds**.
2. Choose **Open SFX folder**, then browse or search your WAV/MP3 effects.
3. **Add** fills the next empty pad. **To selected pad** assigns to the chosen pad. You can also assign an individual audio file or clear the selected pad.
4. Choose a **Pad playback mode**. It saves automatically; **Apply this mode to all pads** applies it to the entire bank.
5. Return with **Back to sound pads**. Click a pad, or highlight it with the controller and press R3. Drag assigned pads to rearrange them; occupied pads swap.

| Mode | How it plays |
|---|---|
| **Restart — default** | Tap once to play through. Every new press restarts it from the beginning. |
| **Toggle** | Press once to play; press again to stop. |
| **Overlap** | Every press adds another hit, up to eight simultaneous hits per pad. |
| **Hold** | Plays while held; release stops it. |

Pads highlight while their audio is active. **Stop all samples** silences the bank. Changing a mode stops the affected pad. Keep imported files in their original locations so saved assignments can find them next time.

## Settings and shortcuts

- **Settings → General:** choose an audio output, set cue hold time, and adjust stick sensitivity and automatic-crossfade duration. Applying an audio-device change saves the session and restarts the app; stop recording first.
- **Settings → SkyDek Mapper:** live Xbox feedback, Normal/Shift assignments, and saved mapping profiles.
- **F11** or the fullscreen button: toggle fullscreen. **Space:** deck play/pause. **C:** cue preview.
- Closing the window keeps SkyDek running in the tray. Use the tray's **Show** or **Exit** commands.

## Preview notes and troubleshooting

This is an early preview, not a finished professional-performance release. Automatic BPM analysis, phase synchronization, and stems separation are not available. Enter BPM manually for tempo matching/beat loops; tempo changes also change pitch. Separate headphone cueing is not available. Test your controller, output, samples, and recording before relying on a live set.

- **Controller not responding:** confirm Windows sees the controller, power it on, center both sticks, and inspect live input in Mapper settings. Confirm the active profile and Normal/Shift layer. Try USB if Bluetooth is unreliable.
- **No sound:** check Master/deck levels, crossfader, Windows output volume, and the selected device. Refresh the output list after connecting an interface, then apply the output and restart.
- **A sample needs a hold:** select that pad's **Restart** mode, or apply it to all pads.
- **A sample is missing:** locate the original file or assign it again. Pads become available after their audio has loaded.
- **Setup cannot install WebView2:** install the Evergreen Runtime from Microsoft's official page, then rerun SkyDek setup.
- **Reporting a problem:** use this repository's Issues tab when enabled. Include your SkyDek version, Windows version, connection type, and clear steps to reproduce. Do not upload private music, recordings, or personal session files.

The preview installer is not digitally signed. Verify the download source and checksum before deciding whether to run a publisher-warning prompt.

## Download verification

SHA-256 for **INSTALLER_NAME**:

```text
INSTALLER_HASH
```

## About this distribution

SkyDek DJ application and branding © 2026 New Ground Enterprise. Proprietary application source is not included or licensed as open source. Third-party components retain their respective licenses; notices and applicable dependency source are in **THIRD-PARTY**. SkyDek DJ is an independent application and is not affiliated with Microsoft, Xbox, or VirtualDJ.

Package contents: Windows setup, this guide, brand asset, third-party notices, and dependency source required for those notices. No personal library, recordings, credentials, internal build documents, or application source is included.
'''
if version == '0.7.0':
    readme = readme.replace('## Download and install', '## New in 0.7.0\n\nA cleaner workspace with matte graphite panels, restrained purple accents, clearer library text, compact effect groups, a collapsible controller guide, and matching settings. The existing controller mappings and sampler playback modes carry forward.\n\n[Get the complete Windows download package](https://github.com/NewGrndCo/SkyDek-DJ/releases/tag/v0.7.0).\n\n## Download and install')
(out/'README.md').write_text(readme.replace('INSTALLER_NAME',public_name).replace('INSTALLER_HASH',digest).replace('0.6.0', version),encoding='utf-8')
# Read executable import names directly, without executing the binary.
data=(root/'src-tauri/target/release/skydek-dj.exe').read_bytes()
pe=struct.unpack_from('<I',data,0x3c)[0]; count=struct.unpack_from('<H',data,pe+6)[0]; optsize=struct.unpack_from('<H',data,pe+20)[0]
opt=pe+24; sections=opt+optsize
def offset(rva):
    for n in range(count):
        pos=sections+n*40
        size,base,rawsize,raw=struct.unpack_from('<IIII',data,pos+8)
        if base <= rva < base+max(size,rawsize): return raw+rva-base
    return rva
imports=[]; rva=struct.unpack_from('<I',data,opt+112+8)[0]; at=offset(rva)
while any(data[at:at+20]):
    name=offset(struct.unpack_from('<I',data,at+12)[0]); imports.append(data[name:data.index(b'\0',name)].decode()); at+=20
archive=out.parent/(out.name+'.zip')
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED) as z:
    for f in sorted(out.rglob('*')):
        if f.is_file() and '.git' not in f.relative_to(out).parts: z.write(f,out.name+'/'+f.relative_to(out).as_posix())
with zipfile.ZipFile(archive) as z:
    assert z.testzip() is None
    assert hashlib.sha256(z.read(out.name+'/'+public_name)).hexdigest()==digest
print(json.dumps({'folder':str(out),'zip':str(archive),'bytes':archive.stat().st_size,'installer_sha256':digest,'rust_packages':len(packages),'missing_license_text':missing,'mpl_sources':[p['name'] for p,_ in mpl],'exe_imports':imports},indent=2))
