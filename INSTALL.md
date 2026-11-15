# Build and install Dito on your Mac

Clone the Dito repository to build and install it on your Mac. If the Mac is managed, confirm that your organization allows locally built apps.

Open Terminal in the checkout and run:

```bash
bash scripts/install-local.sh
```

Choose English, Japanese, Simplified Chinese, or Korean when the script starts. Press Return to use the default, which follows the first language in macOS Language & Region settings and falls back to English for other languages. When run without an interactive terminal, the script uses that default.

The script checks CMake first. It reuses an existing installation or runs `brew install cmake`. If CMake and Homebrew are both unavailable, it exits with setup instructions.

The installer checks that Rust and Cargo meet the minimum version recorded in `Cargo.toml`. It uses the active toolchain if compatible, then checks other installed rustup toolchains. The minimum comes from APIs used by locked dependencies. If no compatible toolchain is installed, the script asks before installing the latest stable Rust with [rustup](https://rust-lang.org/install/). If the macOS SDK is missing, complete Apple's Command Line Tools prompt.

The installer builds and signs the app with your `dito-dev` identity, then installs and opens `~/Applications/Dito.app`. macOS may ask you to approve the signing identity on the first run. Grant Dito microphone and accessibility access when prompted.

Run the same command from the same checkout after pulling an update. The local build profile reuses compilation results in `target/local`.
