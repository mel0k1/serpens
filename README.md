<div align="center">

<img src="assets/serpens-icon.jpg" width="180" alt="Serpens">

# Serpens

Дистрибутив на базе Gentoo с собственным пакетным менеджером flexura, написанным на Rust.

![license](https://img.shields.io/badge/license-GPL--3.0-blueviolet)
![rust](https://img.shields.io/badge/rust-1.74%2B-dea584)
![status](https://img.shields.io/badge/status-experimental-red)

[README in English](README.en.md)

</div>

## Что это

Gentoo построен на сборке из исходников, и в этом его сила. Portage при этом живёт на Python, стартует медленно, а в его bash-скриптовых ебилдах накопилось столько истории, что новичку там тяжело ориентироваться. Serpens сохраняет саму идею: фазовая сборка, профили, USE-флаги, дерево пакетов. Вместо Portage здесь flexura. Рецепты лежат в TOML вместо bash, движок написан на Rust, а баш остаётся только там, где он органичен, то есть в командах сборки.

Проект ранний, и это честно: дерево рецептов экспериментальное, glibc и ядро в нём пока отсутствуют. Как песочница и фундамент для собственных сборок рабочее, для повседневной системы пока бери сам Gentoo.

## flexura

Пакетный менеджер. Команды в стиле подкоманд, у портажевских операций есть алиасы:

| flexura | алиасы | действие | аналог в Portage |
|---|---|---|---|
| `flexura sync` | `update` | клонирует и обновляет репозитории рецептов | `emerge --sync` |
| `flexura install bash` | `emerge`, `add` | разрешает зависимости, собирает, ставит | `emerge bash` |
| `flexura fetch bash` | `download` | только скачивает исходники | `emerge --fetchonly` |
| `flexura remove bash` | `unmerge`, `del` | удаляет пакет по списку файлов | `emerge --unmerge` |
| `flexura upgrade` | | пересобирает всё из world | `emerge --update @world` |
| `flexura depclean` | `clean` | убирает осиротевшие зависимости | `emerge --depclean` |
| `flexura search htop` | `find` | ищет по именам и описаниям | `emerge --search` |
| `flexura info bash` | `show` | показывает рецепт и статус | `equery show` |
| `flexura info` | | сводка о системе: профиль, USE, счётчики | `emerge --info` |
| `flexura list -i` | `ls` | установленные или доступные пакеты | `qlist -I` |
| `flexura deps git` | `tree` | дерево зависимостей | `equery depgraph` |
| `flexura file /usr/bin/git` | `owner` | какой пакет владеет файлом | `equery belongs` |
| `flexura quickpkg bash` | `pkg` | пакует установленный пакет в тарбол | `quickpkg` |
| `flexura world` | | печатает world-файл | `cat /var/lib/portage/world` |

Ограничения версий пишутся в двух формах: `ncurses>=6.0` и `>=ncurses-6.0`, работают `=bash-5.2.21`, `~openssl-3.3`. USE-флаги задаются в `flexura.conf` и в профиле, у рецепта есть свои флаги по умолчанию, условные зависимости объявляются секцией `optional`.

Данные живут по привычным путям: world в `/var/db/flexura/world`, база установленных пакетов в `/var/db/flexura/installed.json`, логи сборки в `/var/log/flexura`, исходники в `/var/cache/flexura/distfiles`, упакованные пакеты в `/var/cache/flexura/packages`.

Чего пока нет: слоты, блокеры, бинарный хост, sandbox для фаз, хуки. Это план, а не дыры в документации.

## Рецепт

Один пакет равен одному .flx файлу в TOML:

```toml
[package]
name = "htop"
version = "3.3.0"
description = "Интерактивный монитор процессов в терминале"
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

Фазы те же, что в Portage: prepare, configure, compile, install. Каждая строка выполняется башем с `-e`, окружение привычное: `$D`, `$WORKDIR`, `$DISTDIR`, `$NPROC`, `$USE`, `$PF`, `$PN`, `$PV`. Для скачанных исходников лучше указать `sha256`, без него flexura предупреждает и ставит как есть.

## Быстрый старт

Нужны Rust 1.74 или новее и баш.

```bash
git clone https://github.com/mel0k1/serpens.git
cd serpens/flexura
cargo build --release
```

Без риска для системы можно всё попробовать через FLEXURA_ROOT, менеджер раскладывает файлы в песочный корень:

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

hello печатает приветствие, serpens-base кладёт os-release и ascii-логотип. Дальше можно поиграть с `remove`, `depclean`, `quickpkg` и убедиться, что файлы удаляются по списку.

## Дерево

```
serpens/
├── flexura/       пакетный менеджер на Rust
├── recipes/       дерево пакетов: base, core, libs, net, tools
├── profiles/      профили x86-64 и x86-64-musl
├── etc/           os-release и пример flexura.conf
├── fastfetch/     конфиг с логотипом
├── neofetch/      то же для neofetch
├── scripts/       mkstage.sh и генератор тестового пакета
└── assets/        иконка и ascii
```

## Брендинг

Иконка лежит в `assets/serpens-icon.jpg`, ascii-версия в `assets/serpens.ascii`:

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

Быстрая проверка одной командой:

```bash
fastfetch --logo assets/serpens.ascii --logo-color magenta
```

Постоянный конфиг для fastfetch лежит в `fastfetch/serpens.jsonc`, скопируй его в `/etc/fastfetch.jsonc` или в `~/.config/fastfetch/config.jsonc`. Для neofetch есть `neofetch/config.conf`: он читает ascii из `/usr/share/serpens/serpens.ascii`, этот файл ставится пакетом serpens-base.

## Профили

Выбираются ключом `profile` в flexura.conf, сейчас профиль задаёт дефолтные USE-флаги:

- `default/x86-64` - widec, unicode, openrc
- `default/x86-64-musl` - widec, unicode, musl, пока декларативный

Позже в профиль переедут выбор libc и тулчейна.

## План

- слоты и блокеры в резолвере
- бинарный хост: формат .ftar с метаданными и проверкой сумм
- sandbox для фаз сборки, хуки до и после
- stage-тарбол через `scripts/mkstage.sh` и установщик на реальный диск
- конвертер ебилдов из ::gentoo в .flx

## Лицензия

GPL-3.0, полный текст в [LICENSE](LICENSE). Автор mel0k1.
