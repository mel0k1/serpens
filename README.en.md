<div align="center">

<img src="assets/serpens-icon.jpg" width="180" alt="Serpens">

# Serpens

A Gentoo-based Linux distribution with its own package manager, flexura, written in Rust.

![license](https://img.shields.io/badge/license-GPL--3.0-blueviolet)
![rust](https://img.shields.io/badge/rust-1.74%2B-dea584)
![ci](https://github.com/mel0k1/serpens/actions/workflows/ci.yml/badge.svg)
![status](https://img.shields.io/badge/status-experimental-red)

[README на русском](README.md)

</div>

## What it is

Gentoo is built around compiling from source, and that is its strength. Portage, meanwhile, runs on Python, starts slowly, and its bash-script ebuilds carry decades of history that is hard to navigate. Serpens keeps the idea: phased builds, profiles, USE flags, a package tree. Portage is replaced by flexura. Recipes live in TOML instead of bash, the engine is written in Rust, and bash stays only where it belongs, inside the build commands themselves.

The project is early and honest about it: the recipe tree is experimental, glibc is not there yet and the system init is busybox. As a playground and a foundation for your own builds it works; for a daily system stick with plain Gentoo for now.

## flexura

The package manager. Subcommand style, with aliases for the Portage operations you know:

| flexura | aliases | action | Portage equivalent |
|---|---|---|---|
| `flexura sync` | `update` | clone and update recipe repositories | `emerge --sync` |
| `flexura install bash` | `emerge`, `add` | resolve deps, build, merge; `-p` shows the plan without building, `-k` reuses a tarball from pkgdir | `emerge bash` |
| `flexura fetch bash` | `download` | download sources only | `emerge --fetchonly` |
| `flexura remove bash` | `unmerge`, `del` | delete a package by its file list; refuses while other packages need it, `-p` for a dry run, `--force` to push through | `emerge --unmerge` |
| `flexura upgrade` | | move world packages up to newer versions | `emerge --update @world` |
| `flexura depclean` | `clean` | drop orphaned dependencies, `-p` lists candidates | `emerge --depclean` |
| `flexura search htop` | `find` | search names and descriptions | `emerge --search` |
| `flexura info bash` | `show` | show a recipe and its status | `equery show` |
| `flexura info` | | system summary: profile, USE, counters | `emerge --info` |
| `flexura list -i` | `ls` | installed or available packages | `qlist -I` |
| `flexura deps git` | `tree` | dependency tree, `-R` reversed — who depends on the package | `equery depgraph`, `equery depends` |
| `flexura file /usr/bin/git` | `owner` | which package owns a file | `equery belongs` |
| `flexura quickpkg bash` | `pkg` | pack an installed package into a tarball | `quickpkg` |
| `flexura verify` | `qcheck` | check installed files against sha256 from the database | `qcheck` |
| `flexura log` | | install history, newest first, `-n` sets the count | `genlop -l` |
| `flexura world` | | print the world file | `cat /var/lib/portage/world` |

Version constraints come in two forms, `ncurses>=6.0` and `>=ncurses-6.0`, plus `=bash-5.2.21` and `~openssl-3.3`. USE flags are set in `flexura.conf` and in the profile, a recipe carries its own defaults, and conditional dependencies go into an `optional` section.

Data lives where Portage users expect it: world in `/var/db/flexura/world`, the installed database in `/var/db/flexura/installed.json` with a sha256 for every file, build logs in `/var/log/flexura`, sources in `/var/cache/flexura/distfiles`, packed packages in `/var/cache/flexura/packages`. On merge flexura warns about files already owned by another package and refreshes the dynamic linker cache in the root.

## Slots and blockers

A recipe declares its own slot, a dependency picks one with a colon, just like in ebuilds:

```toml
[package]
name = "openssl"
version = "1.1.1w"
slot = "1.1"
```

```toml
[dependencies]
build = ["openssl:1.1", ">=ncurses-6.0:6", "zlib>=1.2"]
```

A dependency without a slot accepts any of them. Every database record is keyed as `name:slot`, so `openssl:1.1` and `openssl:3.0` coexist, and `flexura remove openssl:1.1` drops only that slot. When a dependency names a slot that does not exist, the resolver says so directly.

Blockers forbid neighbourhood. `!pkg` forbids pkg in the same install transaction, `!!pkg` also complains when pkg is already on the system:

```toml
[dependencies]
build = ["!oldmail", "!!newmail>=2.0"]
```

The resolver checks blockers against the whole build plan, not just one recipe's dependencies.

## ebuild2flx

A converter from Gentoo ebuilds to .flx recipes. It carries over global variables, multiline DEPEND/RDEPEND with conditional groups `flag? ( ... )`, the src_prepare/configure/compile/install phases, econf, emake and the file installers like dobin, dodoc, dosym, newbin. `use_enable` and `use_with` keep working inside recipes: flexura injects a small shim with those functions into the build environment.

```bash
./ebuild2flx/target/release/ebuild2flx -o recipes/tools htop-3.5.3.ebuild
```

The converter is honest about what it cannot do: atoms with `[use]` conditions are skipped, `|| ( ... )` takes the first choice, pkg_postinst and the python eclasses need manual work. `mirror://` URLs of known mirrors are resolved, the rest ask for hand editing. The sha256 sum has to be computed after conversion.

## Stage and installation

`scripts/mkstage.sh` bootstraps a root: it builds flexura, installs a base set into a fresh root (serpens-base, serpens-boot, busybox, bash, coreutils, ncurses, readline, zlib, grub), writes a config pointing at the repository and packs the tarball:

```bash
./scripts/mkstage.sh serpens-stage-x86_64.tar.gz
```

The tarball carries a ready root with a world file and a working flexura, so after unpacking `flexura install` and `flexura upgrade` work right away. Packages are built with the current host toolchain, so the stage targets x86-64 Linux with glibc; on such hosts it installs without a rebuild.

`scripts/install.sh` puts the stage on a real disk: it partitions GPT with an ESP or a bios boot partition, formats, downloads the tarball (or takes a local one), unpacks it, mounts dev/proc/sys, sets up hostname, timezone and the root password, then unmounts everything and prints what is left to do by hand:

```bash
sudo ./scripts/install.sh --stage ./serpens-stage-x86_64.tar.gz --disk /dev/vda --efi
```

Kernel and bootloader are in the tree: the `linux` recipe builds a bzImage from defconfig (ext4, virtio and iso9660 included) and installs it into /boot with modules, the `grub` recipe builds modules for both EFI and BIOS, serpens-boot provides busybox inittab, rc scripts and the root account. With the `--grub` flag install.sh installs grub from the stage via chroot (or from the host when grub is not built yet), writes fstab with the root UUID and generates grub.cfg.

From a stage and a kernel, `scripts/mkiso.sh` builds a bootable live ISO: an initramfs with a static busybox, at boot the stage is unpacked into RAM and the mounted medium stays at /media/serpens, so install.sh can put the system on disk right from the live session:

```bash
./scripts/mkstage.sh serpens-stage-x86_64.tar.gz --pkgs "serpens-base serpens-boot busybox bash coreutils ncurses readline zlib linux grub"
./scripts/mkiso.sh serpens-stage-x86_64.tar.gz serpens-live-x86_64.iso
```

The image boots both BIOS and EFI; for a local check qemu is enough: `qemu-system-x86_64 -m 2048 -nographic -cdrom serpens-live-x86_64.iso`.

## CI

GitHub Actions builds the workspace under fmt, clippy with `-D warnings` and tests, checks that every recipe parses and resolves, runs a smoke install of the test package, and finally bootstraps a stage tarball with serpens-boot and grub and verifies it by unpacking. The release workflow, triggered by `v*` tags or manually, additionally builds the kernel and a live ISO and boots the system in qemu: the test waits for a login prompt on the serial console, logs in as root and powers the machine off. A release publishes the stage tarball, the flexura and ebuild2flx binaries, the live ISO and sha256sums.

## A recipe

One package, one .flx file, TOML:

```toml
[package]
name = "htop"
version = "3.3.0"
description = "Interactive process monitor for the terminal"
homepage = "https://htop.dev/"
license = "GPL-2.0"

[source]
url = "https://github.com/htop-dev/htop/releases/download/3.3.0/htop-3.3.0.tar.xz"

[dependencies]
build = ["ncurses>=6.0"]

[flags]
default = ["unicode"]

[build]
configure = ["./configure --prefix=/usr"]
compile = ["make -j${NPROC}"]
install = ["make DESTDIR=${D} install"]
```

The phases mirror Portage: prepare, configure, compile, install. Every line runs under `bash -e` with the usual environment: `$D`, `$WORKDIR`, `$S`, `$DISTDIR`, `$NPROC`, `$USE`, `$PF`, `$PN`, `$PV`, `$EROOT`, plus the `use`, `use_enable`, `use_with` shim functions. Add `sha256` for downloaded sources; without it flexura warns and installs as-is.

## Quick start

You need Rust 1.74 or newer, bash, gcc and make.

```bash
git clone https://github.com/mel0k1/serpens.git
cd serpens
cargo build --release
```

The binaries land in `target/release/`. Try it safely with FLEXURA_ROOT; the manager lays everything into a sandboxed root:

```bash
./scripts/mktest-pkg.sh var/flexura-test
mkdir -p var/test-root/etc/flexura
cat > var/test-root/etc/flexura/flexura.conf <<EOF
[main]
repos = ["$PWD/var/flexura-test", "$PWD"]
jobs = 2
EOF
FLEXURA_ROOT=$PWD/var/test-root ./target/release/flexura install hello serpens-base
FLEXURA_ROOT=$PWD/var/test-root ./target/release/flexura list -i
```

hello prints a greeting, serpens-base drops in os-release and the ascii logo. From there, play with `remove`, `depclean` and `quickpkg` and watch files disappear exactly per the recorded list.

## Tree

```
serpens/
├── flexura/       the package manager, in Rust
├── ebuild2flx/    the ebuild to .flx converter
├── recipes/       the package tree: base, core, libs, net, tools
├── profiles/      x86-64 and x86-64-musl profiles
├── etc/           os-release and a sample flexura.conf
├── fastfetch/     config with the logo
├── neofetch/      the same for neofetch
├── scripts/       mkstage.sh, install.sh, mkiso.sh and the test package generator
├── .github/       CI and release pipelines
└── assets/        icon and ascii art
```

## Branding

The icon lives in `assets/serpens-icon.jpg`, the ascii version in `assets/serpens.ascii`:

```
                                                                      
                                                                      
                                                                      
                                                                      
                       .::*###=:                                      
                    .*############.                                   
                   +#####++###+=+###=.                                
                  *###*      *#######.                                
                 =####                              --                
                 -####-   --###########+-       .*+-..=+#             
                  +####+-*###*+.     =*#####.   #-.###* +#.           
                    +#####+= -++*##+++.  :=###: #-*#=.#= #=           
                   :#=+*#######*++++*####:  +###-*=-=#+ +#.           
                  =###  +###.          -###: :####.   *#+             
                 :##+  *##:  +########=: -##*  ##*..-:                
                 =#*  ###  +##*.    .+##- :##+  ##.                   
                .##* .##: :##. *####- :##* +##: ###                   
                .##* =##: -## .#*:-##: =## +##- ###                   
                .##*  ##- :##*.   *##..### +##: ###                   
                 =##- :##* .#######*. *## -##+ :##.                   
                  *##: :*#*=   ..   -*##:=##+ :##-                    
                   ####   #############=###: :##* *###.               
                   .*###+-  --=##########:  =##.++.  .-#:             
                 .*#################*+.  :*##*#.#-.==# -=             
                .#####...+####**+.  =**###+. :#=:####:.#:             
                *###:       :-=*####*=-:      .-#+===*-.              
                ###*                                                  
                .###+.       :+**-                                    
                  -##########==-:-#*                                  
                                   *                                  
                                  +:                                  
                                                                      
                                                                      
                                                                      
                                                                      
```

A one-shot check:

```bash
fastfetch --logo assets/serpens.ascii --logo-color magenta
```

The persistent fastfetch config is `fastfetch/serpens.jsonc`, copy it to `/etc/fastfetch.jsonc` or `~/.config/fastfetch/config.jsonc`. For neofetch there is `neofetch/config.conf`: it reads the ascii from `/usr/share/serpens/serpens.ascii`, a file installed by the serpens-base package.

## Profiles

Selected with the `profile` key in flexura.conf; today a profile sets default USE flags:

- `default/x86-64` - widec, unicode, openrc
- `default/x86-64-musl` - widec, unicode, musl, declarative for now

libc selection and the toolchain will move into profiles later.

## Roadmap

- a binary host: the .ftar format with metadata and checksum verification
- a sandbox for build phases, pre and post hooks
- a glibc recipe, so the stage stops depending on the host toolchain
- build sub-slots

## License

GPL-3.0, full text in [LICENSE](LICENSE). Author mel0k1.
