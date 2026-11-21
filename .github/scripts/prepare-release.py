import base64
import json
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path
from urllib.parse import quote


# Each artifact directory is named after its Rust target. Updater keys also
# identify the installer format so Linux AppImages and .debs stay independent.
PACKAGES = (
    ("windows-x86_64-nsis", "x86_64-pc-windows-msvc", "*-setup.exe"),
    ("windows-aarch64-nsis", "aarch64-pc-windows-msvc", "*-setup.exe"),
    ("darwin-x86_64-app", "x86_64-apple-darwin", "*.app.tar.gz"),
    ("darwin-aarch64-app", "aarch64-apple-darwin", "*.app.tar.gz"),
    ("linux-x86_64-appimage", "x86_64-unknown-linux-gnu", "*.AppImage"),
    ("linux-x86_64-deb", "x86_64-unknown-linux-gnu", "*.deb"),
    ("linux-aarch64-deb", "aarch64-unknown-linux-gnu", "*.deb"),
    ("linux-armv7-deb", "armv7-unknown-linux-gnueabihf", "*.deb"),
)


def find_package(artifacts, target, pattern):
    files = list((artifacts / f"golden-launcher-{target}").rglob(pattern))
    if len(files) != 1:
        raise ValueError(f"Expected one {pattern} for {target}, found {len(files)}")
    return files[0]


def stage(source, output, name):
    destination = output / name
    if destination.exists():
        raise ValueError(f"Duplicate release asset {name}")
    shutil.copyfile(source, destination)


def verify_signature(package, version, public_key, scratch):
    signature_path = package.with_name(package.name + ".sig")
    signature = signature_path.read_text().strip()
    decoded = base64.b64decode(signature, validate=True)
    decoded_signature = scratch / "package.minisig"
    decoded_signature.write_bytes(decoded)
    subprocess.run(
        ["minisign", "-Vq", "-m", package, "-p", public_key, "-x", decoded_signature],
        check=True,
    )
    trusted_comment = decoded.decode().splitlines()[2]
    if f"version:{version}" not in trusted_comment.split("\t"):
        raise ValueError(f"Signature for {package.name} does not bind version {version}")
    return signature


def main(tag, artifacts, output):
    config = json.loads(Path("src-tauri/tauri.conf.json").read_text())
    npm = json.loads(Path("package.json").read_text())
    cargo = tomllib.loads(Path("src-tauri/Cargo.toml").read_text())
    version = config["version"]
    if (
        not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag)
        or tag != f"v{version}"
        or npm["version"] != version
        or cargo["package"]["version"] != version
    ):
        raise ValueError("The release tag and Tauri, npm, and Cargo versions must agree")

    output.mkdir()
    platforms = {}
    releases = f"https://github.com/Golden-Geek/Golden-Launcher/releases/download/{tag}"
    with tempfile.TemporaryDirectory() as temporary:
        scratch = Path(temporary)
        public_key = scratch / "updater.pub"
        public_key.write_bytes(base64.b64decode(config["plugins"]["updater"]["pubkey"]))
        for key, target, pattern in PACKAGES:
            package = find_package(artifacts, target, pattern)
            signature = verify_signature(package, version, public_key, scratch)
            name = package.name.replace(" ", ".")
            if key.startswith("darwin-"):
                installer = find_package(artifacts, target, "*.dmg")
                stage(installer, output, installer.name.replace(" ", "."))
                name = f"Golden.Launcher_{version}_{target}.app.tar.gz"
            stage(package, output, name)
            platforms[key] = {"url": f"{releases}/{quote(name)}", "signature": signature}

    manifest = {"version": version, "platforms": platforms}
    (output / "latest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main(sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3]))
