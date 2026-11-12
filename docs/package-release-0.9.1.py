"""Prepare a NEW distribution only after the parent approves ready inputs.

Python 3.12+ and Cargo/MSVC required. Never edits the public Git checkout.
No imports from the old, side-effecting packager. Fails closed on missing materials.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.9.1"
TARGET = "x86_64-pc-windows-msvc"
NAME = f"SkyDek-DJ-{VERSION}-Windows"
SOURCE = f"SkyDek-MP3-Encoder-{VERSION}-Source.zip"
FORBIDDEN = {".git", "target", "node_modules", ".env", "sessions", "recordings", "music"}


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def run(args, cwd):
    result = subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True,
                            encoding="utf-8", errors="strict")
    return result.stdout


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def files(folder):
    for p in sorted(folder.rglob("*")):
        require(not p.is_symlink() and not p.is_junction(), f"Linked source forbidden: {p.name}")
        if p.is_file():
            yield p


def archive(folder, dest):
    # Stable ordering, timestamps and permissions; content changes still change hashes.
    with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for p in files(folder):
            rel = p.relative_to(folder).as_posix()
            parts = Path(rel).parts
            require(parts[0].lower() not in FORBIDDEN and not any(x.lower() in {".git", ".env"} for x in parts),
                    f"Forbidden archive member: {rel}")
            info = zipfile.ZipInfo(rel, (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            z.writestr(info, p.read_bytes())
    with zipfile.ZipFile(dest) as z:
        require(z.testzip() is None, "ZIP integrity failure")
        for p in files(folder):
            require(hashlib.sha256(z.read(p.relative_to(folder).as_posix())).hexdigest() == sha(p),
                    "ZIP content hash mismatch")


def metadata(folder, offline=False, windows=False):
    cmd = ["cargo", "metadata", "--format-version", "1", "--locked"]
    if offline:
        cmd += ["--offline"]
    if windows:
        cmd += ["--filter-platform", TARGET]
    return json.loads(run(cmd, folder))


def resolved(meta):
    ids = {n["id"] for n in meta["resolve"]["nodes"]}
    return [p for p in meta["packages"] if p["id"] in ids]


def licenses(package):
    folder = Path(package["manifest_path"]).parent
    found = [p for p in folder.iterdir() if p.is_file() and
             p.name.lower().startswith(("license", "licence", "copying", "notice"))]
    if package.get("license_file"):
        p = (folder / package["license_file"]).resolve()
        require(p.is_relative_to(folder.resolve()), "License outside package")
        if p.is_file() and p not in found:
            found.append(p)
    if not found:
        vcs = folder / ".cargo_vcs_info.json"
        commit = json.loads(vcs.read_text())["git"]["sha1"] if vcs.exists() else ""
        repo = (package.get("repository") or "").removesuffix(".git")
        if repo.startswith("https://github.com/") and commit:
            key = repo.removeprefix("https://github.com/").replace("/", "-")
            found = list((ROOT / "docs/dependency-license-cache").glob(f"{key}-{commit}-*"))
        if not found and package.get("license") == "MPL-2.0":
            p = ROOT / "docs/dependency-license-cache/MPL-2.0.txt"
            if p.exists():
                found = [p]
    require(found, f"Missing license text: {package['name']} {package['version']}")
    return found


def notices(packages):
    records = []
    for p in sorted(packages, key=lambda p: (p["name"], p["version"])):
        records.append(f"\n{'=' * 72}\n{p['name']} {p['version']}\n"
                       f"License: {p.get('license') or 'See included text'}\n"
                       f"Upstream: {p.get('repository') or 'https://crates.io/crates/' + p['name']}\n")
        for f in licenses(p):
            records.append(f"\n--- {f.name} ---\n" + f.read_text(encoding="utf-8", errors="replace"))
    return "".join(records)


def prepare(args):
    require(os.name == "nt", "Generate metadata and verify builds on Windows")
    helper = ROOT / "encoder-helper"
    installer = ROOT / f"src-tauri/target/release/bundle/nsis/SkyDek DJ_{VERSION}_x64-setup.exe"
    helper_exe = ROOT / "src-tauri/resources/encoder/skydek-mp3-encoder.exe"
    main_exe = ROOT / "src-tauri/target/release/skydek-dj.exe"
    dest = ROOT / "release" / NAME
    final_zip = dest.parent / (dest.name + ".zip")
    require(not dest.exists() and not final_zip.exists(), "Release output already exists; never overwrite")
    for p in (installer, helper_exe, main_exe, helper / "Cargo.lock", helper / "LICENSE"):
        require(p.is_file(), f"Missing ready input: {p.relative_to(ROOT)}")
    for manifest in (ROOT / "package.json", ROOT / "src-tauri/tauri.conf.json"):
        require(json.loads(manifest.read_text())["version"] == VERSION, "Version mismatch")
    require(tomllib.loads((ROOT / "src-tauri/Cargo.toml").read_text())["package"]["version"] == VERSION,
            "Cargo version mismatch")
    # Tauri restores the main executable's bundle marker after generating NSIS,
    # which changes its mtime. Installed byte comparison below verifies that app.
    require(installer.stat().st_mtime >= helper_exe.stat().st_mtime,
            "Installer predates helper; rebuild it")
    main = metadata(ROOT / "src-tauri", windows=True)  # fresh, never stale TEMP metadata
    main_packages = resolved(main)
    require(not any("lame" in p["name"].lower() or p["name"] == "libloading" for p in main_packages),
            "Main resolved graph still includes LAME or libloading")
    require(all(p["source"] or p["id"] == main["resolve"]["root"] for p in main_packages),
            "Unreviewed local main dependency")
    main_notice = notices([p for p in main_packages if p["source"]])
    with tempfile.TemporaryDirectory(prefix="skydek-release-") as temp:
        work = Path(temp)
        out = work / NAME
        third = out / "THIRD-PARTY"
        third.mkdir(parents=True)
        source = work / "helper-source"
        source.mkdir()
        # This dedicated crate is the ONLY authored source boundary. Never copy the app.
        for p in files(helper):
            rel = p.relative_to(helper)
            if any(x.lower() in {"target", ".git"} for x in rel.parts):
                continue
            require(not any(x.lower() in FORBIDDEN or x.startswith(".env") for x in rel.parts),
                    f"Private helper input: {rel}")
            require(p.suffix.lower() not in {".exe", ".dll", ".pdb", ".wav", ".mp3"},
                    f"Unexpected binary/media in helper source: {rel}")
            payload = p.read_bytes()
            require(not re.search(rb"(?i)([a-z]:[\\/]users[\\/]|/Users/|/home/)", payload),
                    f"Private absolute path in helper source: {rel}")
            target = source / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(p, target)
        manifest = tomllib.loads((source / "Cargo.toml").read_text())
        require(manifest["package"]["name"] == "skydek-mp3-encoder", "Unexpected helper crate")
        require(manifest["package"].get("license") == "MIT", "Helper authored source must declare MIT")
        require("MIT License" in (source / "LICENSE").read_text(), "Missing helper MIT text")
        # No platform filter: all lockfile dependencies, including foreign targets.
        config = run(["cargo", "vendor", "--locked", "vendor"], source)
        require(str(source) not in config and str(helper) not in config, "Nonportable vendor config")
        write(source / ".cargo/config.toml", config)
        helper_meta = metadata(source, offline=True)
        helper_packages = resolved(helper_meta)
        require(all(p["source"] or p["id"] == helper_meta["resolve"]["root"] for p in helper_packages),
                "Helper has external path dependencies; source would be incomplete")
        helper_notice = notices(helper_packages)
        require(any(p["name"] == "mp3lame-sys" for p in helper_packages), "Native encoder source absent")
        native = list((source / "vendor").glob("mp3lame-sys*/lame*"))
        require(any(p.is_dir() and list(p.rglob("*.c")) and list(p.rglob("COPYING*")) for p in native),
                "Full native LAME source/COPYING absent")
        for name in ("GPL-3.0.txt", "LGPL-3.0.txt"):
            p = ROOT / "src-tauri/resources/licenses" / name
            require(p.is_file(), f"Missing library license: {name}")
            require(p.stat().st_size > 7000, f"Incomplete library license: {name}")
            (source / "licenses").mkdir(exist_ok=True)
            shutil.copyfile(p, source / "licenses" / name)
        write(source / "THIRD-PARTY-NOTICES.txt", helper_notice)
        require((source / "README.md").is_file(), "Helper build/protocol README missing")
        write(source / "BUILD-OFFLINE.md", """# Build the separate MP3 encoder

This archive contains only the MIT-licensed encoder helper's authored source,
its full locked vendored dependencies, native LAME C source and upstream licenses.
Dependency licenses apply to their own files. Proprietary SkyDek DJ source is excluded.

On Windows x64 install Rust (MSVC host) and Visual Studio C++ Build Tools with a
Windows SDK. A Rust toolchain meeting every locked dependency's rust-version is
required; toolchains and the Windows SDK are not vendored. From this extracted
directory, with no network access required by Cargo:

    cargo metadata --offline --locked --format-version 1
    cargo build --release --offline --locked --target x86_64-pc-windows-msvc

The result is target/x86_64-pc-windows-msvc/release/skydek-mp3-encoder.exe.
See README.md for the raw f32LE stdin protocol, arguments and error behavior.
Exit SkyDek DJ before replacing the installed resources/encoder/skydek-mp3-encoder.exe
with your interface-compatible build. Keep a backup of the original executable.
No proprietary application objects or relinking are needed to replace this process.
You may modify and rebuild the helper and its libraries under their own licenses;
reverse engineering needed to debug modifications of those libraries is permitted.
The main app and branding are not covered by this helper source license.

Archive metadata is normalized for repeatable ZIP generation. Exact byte-for-byte
executable reproduction also depends on toolchain, environment and linker versions.
""")
        write(source / "BUILD-TOOLS.txt", run(["rustc", "-Vv"], source) + run(["cargo", "-V"], source))
        source_zip = third / SOURCE
        archive(source, source_zip)
        # Verify the actual downloadable source, not a working tree with hidden inputs.
        extracted = work / "rebuild"
        with zipfile.ZipFile(source_zip) as z:
            z.extractall(extracted)
        metadata(extracted, offline=True)
        env_before = os.environ.get("CARGO_TARGET_DIR")
        try:
            os.environ["CARGO_TARGET_DIR"] = str(work / "rebuild-target")
            run(["cargo", "build", "--release", "--offline", "--locked", "--target", TARGET], extracted)
        finally:
            if env_before is None:
                os.environ.pop("CARGO_TARGET_DIR", None)
            else:
                os.environ["CARGO_TARGET_DIR"] = env_before
        # Parent has just installed this setup and run installed native verification.
        installed = Path(args.installed_root)
        require(sha(installed / "resources/encoder" / helper_exe.name) == sha(helper_exe),
                "Installed helper binary mismatch")
        def bundle_normalized(path):
            return path.read_bytes().replace(b"__TAURI_BUNDLE_TYPE_VAR_NSS", b"__TAURI_BUNDLE_TYPE_VAR_UNK")
        require(bundle_normalized(installed / main_exe.name) == bundle_normalized(main_exe),
                "Installed main binary mismatch beyond Tauri's NSIS bundle marker")
        # MPL sources only; no main-tree vendor operation or proprietary source copy.
        mpl = work / "mpl"
        mpl.mkdir()
        for p in main_packages:
            if "MPL" in (p.get("license") or ""):
                shutil.copytree(Path(p["manifest_path"]).parent, mpl / f"{p['name']}-{p['version']}")
        require(any(mpl.iterdir()), "Expected MPL dependency source missing")
        archive(mpl, third / "MPL-DEPENDENCY-SOURCES.zip")
        lock = json.loads((ROOT / "package-lock.json").read_text())
        npm = []
        for name, entry in lock["packages"].items():
            if not name or entry.get("dev"):
                continue
            folder = ROOT / name
            require(folder.is_dir(), f"Missing installed npm dependency: {name}")
            package = json.loads((folder / "package.json").read_text())
            require(package["version"] == entry["version"], f"Stale npm dependency: {name}")
            package.update(manifest_path=str(folder / "package.json"))
            npm.append(package)
        write(third / "NOTICES.txt", "SkyDek DJ: resolved Windows main dependencies and build components.\n"
              "Inclusion does not imply runtime inclusion. Each component retains its license.\n"
              + main_notice + notices(npm) + "\nSEPARATE ENCODER HELPER\n" + helper_notice)
        write(third / "README.md", "# Third-party materials\n\nNOTICES.txt inventories resolved Windows main dependencies, frontend runtime dependencies and the separate encoder. MPL-DEPENDENCY-SOURCES.zip supplies main MPL dependency source. " + SOURCE + " supplies complete helper source, locked vendor tree and native LAME source. These materials do not license proprietary application source or branding.\n")
        public_installer = f"{NAME}-Setup.exe"
        shutil.copyfile(installer, out / public_installer)
        shutil.copyfile(helper_exe, third / helper_exe.name)
        (out / "assets").mkdir()
        shutil.copyfile(ROOT / "public/brand.png", out / "assets/brand.png")
        shutil.copyfile(ROOT / "docs/public-README-0.9.1.md", out / "README.md")
        shutil.copyfile(ROOT / "docs/GITHUB-RELEASE-0.9.1.md", out / "RELEASE-NOTES.md")
        write(out / "SOURCE-CODE-LICENSE.md", "# Source-code license scope\n\nOnly authored encoder-helper source is licensed under MIT (the full text is in the source ZIP). Vendored Rust and native LAME dependencies retain their own licenses, including LGPL; see the included texts and notices. SkyDek DJ application source is proprietary, excluded, and not licensed by the helper's MIT license. Application and branding © 2026 New Ground Enterprise. Nothing here restricts rights granted by third-party licenses, including library modification and debugging rights described in BUILD-OFFLINE.md.\n")
        write(out / "SHA256SUMS.txt", "".join(f"{sha(p)}  {p.relative_to(out).as_posix()}\n" for p in files(out)))
        staged_zip = work / f"{NAME}.zip"
        archive(out, staged_zip)
        # Publish locally only after every check succeeds; destinations are new paths.
        shutil.copytree(out, dest)
        shutil.copyfile(staged_zip, final_zip)
        write(ROOT / "release" / f"{NAME}.zip.sha256", f"{sha(final_zip)}  {final_zip.name}\n")
        print(json.dumps({"folder": str(dest), "zip_sha256": sha(final_zip),
                          "source_sha256": sha(dest / "THIRD-PARTY" / SOURCE),
                          "main_graph_no_lame_or_libloading": True,
                          "source_offline_build": True, "installer_bytes_verified": True}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ready-inputs", action="store_true", required=True,
                        help="Parent has approved final installer/helper/source inputs")
    parser.add_argument("--installed-root", required=True, help="Freshly installed and natively verified application directory")
    prepare(parser.parse_args())
