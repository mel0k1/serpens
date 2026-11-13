<div align="center">

<img src="assets/serpens-icon.jpg" width="180" alt="Serpens">

# Serpens

A Gentoo-based Linux distribution with its own package manager, flexura, written in Rust.

![license](https://img.shields.io/badge/license-GPL--3.0-blueviolet)
![rust](https://img.shields.io/badge/rust-1.74%2B-dea584)
![status](https://img.shields.io/badge/status-experimental-red)

[README на русском](README.md)

</div>

## What it is

Gentoo is built around compiling from source, and that is its strength. Portage, meanwhile, runs on Python, starts slowly, and its bash-script ebuilds carry decades of history that is hard to navigate. Serpens keeps the idea: phased builds, profiles, USE flags, a package tree. Portage is replaced by flexura. Recipes live in TOML instead of bash, the engine is written in Rust, and bash stays only where it belongs, inside the build commands themselves.

The project is early and honest about it: the recipe tree is experimental, glibc and the kernel are not there yet. As a playground and a foundation for your own builds it works; for a daily system stick with plain Gentoo for now.

## flexura

The package manager. Subcommand style, with aliases for the Portage operations you know:

| flexura | aliases | action | Portage equivalent |
|---|---|---|---|
| `flexura sync` | `update` | clone and update recipe repositories | `emerge --sync` |
| `flexura install bash` | `emerge`, `add` | resolve deps, build, merge | `emerge bash` |
| `flexura fetch bash` | `download` | download sources only | `emerge --fetchonly` |
| `flexura remove bash` | `unmerge`, `del` | delete a package by its file list | `emerge --unmerge` |
| `flexura upgrade` | | rebuild everything from world | `emerge --update @world` |
| `flexura depclean` | `clean` | drop orphaned dependencies | `emerge --depclean` |
| `flexura search htop` | `find` | search names and descriptions | `emerge --search` |
| `flexura info bash` | `show` | show a recipe and its status | `equery show` |
| `flexura info` | | system summary: profile, USE, counters | `emerge --info` |
| `flexura list -i` | `ls` | installed or available packages | `qlist -I` |
| `flexura deps git` | `tree` | dependency tree | `equery depgraph` |
| `flexura file /usr/bin/git` | `owner` | which package owns a file | `equery belongs` |
| `flexura quickpkg bash` | `pkg` | pack an installed package into a tarball | `quickpkg` |
| `flexura world` | | print the world file | `cat /var/lib/portage/world` |

Version constraints come in two forms, `ncurses>=6.0` and `>=ncurses-6.0`, plus `=bash-5.2.21` and `~openssl-3.3`. USE flags are set in `flexura.conf` and in the profile, a recipe carries its own defaults, and conditional dependencies go into an `optional` section.

Data lives where Portage users expect it: world in `/var/db/flexura/world`, the installed database in `/var/db/flexura/installed.json`, build logs in `/var/log/flexura`, sources in `/var/cache/flexura/distfiles`, packed packages in `/var/cache/flexura/packages`.

Not there yet: slots, blockers, a binary host, a sandbox for build phases, hooks. That is the roadmap, not a documentation gap.

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

The phases mirror Portage: prepare, configure, compile, install. Every line runs under `bash -e` with the usual environment: `$D`, `$WORKDIR`, `$DISTDIR`, `$NPROC`, `$USE`, `$PF`, `$PN`, `$PV`. Add `sha256` for downloaded sources; without it flexura warns and installs as-is.

## Quick start

You need Rust 1.74 or newer and bash.

```bash
git clone https://github.com/mel0k1/serpens.git
cd serpens/flexura
cargo build --release
```

Try it safely with FLEXURA_ROOT; the manager lays everything into a sandboxed root:

```bash
cd serpens
./scripts/mktest-pkg.sh var/flexura-test
mkdir -p var/test-root/etc/flexura
cat > var/test-root/etc/flexura/flexura.conf <<EOF
[main]
repos = ["$PWD/var/flexura-test", "$PWD/recipes"]
jobs = 2
EOF
FLEXURA_ROOT=$PWD/var/test-root ./flexura/target/release/flexura install hello serpens-base
FLEXURA_ROOT=$PWD/var/test-root ./flexura/target/release/flexura list -i
```

hello prints a greeting, serpens-base drops in os-release and the ascii logo. From there, play with `remove`, `depclean` and `quickpkg` and watch files disappear exactly per the recorded list.

## Tree

```
serpens/
├── flexura/       the package manager, in Rust
├── recipes/       the package tree: base, core, libs, net, tools
├── profiles/      x86-64 and x86-64-musl profiles
├── etc/           os-release and a sample flexura.conf
├── fastfetch/     config with the logo
├── neofetch/      the same for neofetch
├── scripts/       mkstage.sh and the test package generator
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

- slots and blockers in the resolver
- a binary host: the .ftar format with metadata and checksum verification
- a sandbox for build phases, pre and post hooks
- a stage tarball via `scripts/mkstage.sh` and an installer for real disks
- a converter from ::gentoo ebuilds to .flx

## License

GPL-3.0, full text in [LICENSE](LICENSE). Author mel0k1.
