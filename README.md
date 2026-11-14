<div align="center">

<img src="assets/serpens-icon.jpg" width="180" alt="Serpens">

# Serpens

Дистрибутив на базе Gentoo с собственным пакетным менеджером flexura, написанным на Rust.

![license](https://img.shields.io/badge/license-GPL--3.0-blueviolet)
![rust](https://img.shields.io/badge/rust-1.74%2B-dea584)
![ci](https://github.com/mel0k1/serpens/actions/workflows/ci.yml/badge.svg)
![status](https://img.shields.io/badge/status-experimental-red)

[README in English](README.en.md)

</div>

## Что это

Gentoo построен на сборке из исходников, и в этом его сила. Portage при этом живёт на Python, стартует медленно, а в его bash-скриптовых ебилдах накопилось столько истории, что новичку там тяжело ориентироваться. Serpens сохраняет саму идею: фазовая сборка, профили, USE-флаги, дерево пакетов. Вместо Portage здесь flexura. Рецепты лежат в TOML вместо bash, движок написан на Rust, а баш остаётся только там, где он органичен, то есть в командах сборки.

Проект ранний, и это честно: дерево рецептов экспериментальное, glibc в нём пока отсутствует, системный init пока busybox. Как песочница и фундамент для собственных сборок рабочее, для повседневной системы пока бери сам Gentoo.

## flexura

Пакетный менеджер. Команды в стиле подкоманд, у портажевских операций есть алиасы:

| flexura | алиасы | действие | аналог в Portage |
|---|---|---|---|
| `flexura sync` | `update` | клонирует и обновляет репозитории рецептов | `emerge --sync` |
| `flexura install bash` | `emerge`, `add` | разрешает зависимости, собирает, ставит; `-p` показывает план без сборки, `-k` берёт готовый тарбол из pkgdir, `-1` не трогает world | `emerge bash` |
| `flexura fetch bash` | `download` | только скачивает исходники | `emerge --fetchonly` |
| `flexura remove bash` | `unmerge`, `del` | удаляет пакет по списку файлов; отказывается снимать пакет, который нужен другим, `-p` черновик, `--force` продавливает | `emerge --unmerge` |
| `flexura upgrade` | | поднимает пакеты из world до старших версий | `emerge --update @world` |
| `flexura depclean` | `clean` | убирает осиротевшие зависимости, `-p` показывает кандидатов | `emerge --depclean` |
| `flexura search htop` | `find` | ищет по именам и описаниям | `emerge --search` |
| `flexura info bash` | `show` | показывает рецепт и статус | `equery show` |
| `flexura info` | | сводка о системе: профиль, USE, счётчики | `emerge --info` |
| `flexura list -i` | `ls` | установленные или доступные пакеты | `qlist -I` |
| `flexura deps git` | `tree` | дерево зависимостей, `-R` наоборот — кто зависит от пакета | `equery depgraph`, `equery depends` |
| `flexura file /usr/bin/git` | `owner` | какой пакет владеет файлом | `equery belongs` |
| `flexura quickpkg bash` | `pkg` | пакует установленный пакет в тарбол | `quickpkg` |
| `flexura verify` | `qcheck` | сверяет установленные файлы с sha256 из базы | `qcheck` |
| `flexura log` | | история установок, свежие сверху, `-n` задаёт сколько | `genlop -l` |
| `flexura world` | | печатает world-файл | `cat /var/lib/portage/world` |

Ограничения версий пишутся в двух формах: `ncurses>=6.0` и `>=ncurses-6.0`, работают `=bash-5.2.21`, `~openssl-3.3`. USE-флаги задаются в `flexura.conf` и в профиле, у рецепта есть свои флаги по умолчанию, условные зависимости объявляются секцией `optional`. Точечные переопределения живут в `/etc/flexura/package.use`: строка `openssl -doc +static` меняет флаги одного пакета, ключ `openssl:1.1` действует только на указанный слот. В `/etc/flexura/package.mask` прячут версии: строка `>=openssl-3.3` заставляет резолвер молча выбрать старшую немаскированную, а если замаскированы все кандидаты, установка честно падает.

При установке flexura предупреждает о файлах, которые уже принадлежат другому пакету, и обновляет кэш динамического линковщика в корне. Файлы под `etc/` защищены: если меняешься содержимое, старый вариант остаётся на месте, новый кладётся рядом с суффиксом `.flexura-new`. Рецепт может нести секцию `[messages] post = ["..."]` — строки печатаются после установки, как einfo в ебилдах.

Данные живут по привычным путям: world в `/var/db/flexura/world`, база установленных пакетов в `/var/db/flexura/installed.json` с sha256 каждого файла, логи сборки в `/var/log/flexura`, исходники в `/var/cache/flexura/distfiles`, упакованные пакеты в `/var/cache/flexura/packages`. Тарбол quickpkg хранит метаданные (версию, слот, USE) в файле `.flexura-meta.toml`, так что `install -k` восстанавливает пакет ровно в том виде, в каком он был запакован.

## Слоты и блокеры

Рецепт объявляет свой слот, зависимость выбирает слот через двоеточие, как в ебилдах:

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

Зависимость без слота принимает любой. В базе пакетов каждая запись хранится под ключом `имя:слот`, так что `openssl:1.1` и `openssl:3.0` живут рядом, а `flexura remove openssl:1.1` снимает только указанный слот. Если слот в зависимости не существует, резолвер говорит об этом прямо.

Блокеры ставят запрет на соседство. `!pkg` запрещает pkg в той же транзакции установки, `!!pkg` ещё и ругается, если pkg уже стоит в системе:

```toml
[dependencies]
build = ["!oldmail", "!!newmail>=2.0"]
```

Резолвер проверяет блокеры против всего плана сборки, а не только против зависимостей одного рецепта.

## ebuild2flx

Конвертер ебилдов в рецепты .flx. Переносит глобальные переменные, многострочные DEPEND/RDEPEND с условными группами `flag? ( ... )`, фазы src_prepare/configure/compile/install, команду econf, emake и ставильщики файлов вроде dobin, dodoc, dosym, newbin. `use_enable` и `use_with` продолжают работать в рецептах: flexura подкладывает в сборочное окружение маленький shim с этими функциями.

```bash
./ebuild2flx/target/release/ebuild2flx -o recipes/tools htop-3.5.3.ebuild
```

Конвертер честно предупреждает о том, чего не умеет: атомы с `[use]`-условиями, `|| ( ... )` берётся первым вариантом, pkg_postinst и питоньи eclass'ы требуют ручной доработки. URL из `mirror://` известных зеркал подставляются, остальные просят правки. Сумму sha256 после конвертации надо посчитать самому.

## Stage и установка

`scripts/mkstage.sh` бутстрапит корень: собирает flexura, ставит в свежий корень базовый набор (serpens-base, serpens-boot, busybox, bash, coreutils, ncurses, readline, zlib, grub), пишет конфиг с репозиторием и пакует тарбол:

```bash
./scripts/mkstage.sh serpens-stage-x86_64.tar.gz
```

В тарболе готовый корень с world-файлом и рабочим flexura, так что после распаковки `flexura install` и `flexura upgrade` работают сразу. Сборка идёт тулчейном текущего хоста, поэтому stage рассчитан на x86-64 Linux с glibc, на подобных хостах ставится без пересборки.

`scripts/install.sh` ставит stage на реальный диск: размечает GPT с ESP или с bios-загрузочным разделом, форматирует, качает тарбол (или берёт локальный), распаковывает, монтирует dev/proc/sys, настраивает hostname, timezone и пароль, потом отмонтирует всё и печатает, что осталось руками:

```bash
sudo ./scripts/install.sh --stage ./serpens-stage-x86_64.tar.gz --disk /dev/vda --efi
```

Ядро и загрузчик есть в дереве: рецепт `linux` собирает bzImage с defconfig (включены ext4, virtio, iso9660), ядро и модули попадают в /boot; рецепт `grub` содержит модули для EFI и BIOS сразу после сборки; serpens-boot кладёт inittab, rc-скрипты busybox и учётку root. С ключом `--grub` install.sh ставит grub из stage через chroot (или берёт его с хоста, если grub ещё не собран), пишет fstab с UUID корня и генерирует grub.cfg.

Из stage и ядра `scripts/mkiso.sh` собирает загрузочный live-ISO: initramfs со static busybox, при загрузке stage разворачивается в RAM, смонтированный носитель остаётся в /media/serpens, так что install.sh может поставить систему на диск прямо из live-сессии:

```bash
./scripts/mkstage.sh serpens-stage-x86_64.tar.gz --pkgs "serpens-base serpens-boot busybox bash coreutils ncurses readline zlib linux grub"
./scripts/mkiso.sh serpens-stage-x86_64.tar.gz serpens-live-x86_64.iso
```

Полученный образ грузится и в BIOS, и в EFI, для локальной проверки хватит qemu: `qemu-system-x86_64 -m 2048 -nographic -cdrom serpens-live-x86_64.iso`.

## CI

GitHub Actions собирает workspace под fmt, clippy с `-D warnings`, тесты, проверяет, что все рецепты парсятся и резолвятся, гоняет smoke-установку тестового пакета и в конце бутстрапит stage-тарбол с serpens-boot и grub, проверяя его распаковкой. Workflow release на тегах `v*` или по ручному запуску дополнительно компилирует ядро, пакует live-ISO и грузит систему в qemu: тест ждёт приглашение login на последовательной консоли, логинится root-ом и выключает машину. Релиз публикует тарбол stage, бинарники flexura и ebuild2flx, live-ISO и sha256sums.

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

Фазы те же, что в Portage: prepare, configure, compile, install. Каждая строка выполняется башем с `-e`, окружение привычное: `$D`, `$WORKDIR`, `$S`, `$DISTDIR`, `$NPROC`, `$USE`, `$PF`, `$PN`, `$PV`, `$EROOT`, плюс функции `use`, `use_enable`, `use_with` из shim. Для скачанных исходников лучше указать `sha256`, без него flexura предупреждает и ставит как есть.

## Быстрый старт

Нужны Rust 1.74 или новее, баш, gcc и make.

```bash
git clone https://github.com/mel0k1/serpens.git
cd serpens
cargo build --release
```

Бинарники появятся в `target/release/`. Без риска для системы можно всё попробовать через FLEXURA_ROOT, менеджер раскладывает файлы в песочный корень:

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

hello печатает приветствие, serpens-base кладёт os-release и ascii-логотип. Дальше можно поиграть с `remove`, `depclean`, `quickpkg` и убедиться, что файлы удаляются по списку.

## Дерево

```
serpens/
├── flexura/       пакетный менеджер на Rust
├── ebuild2flx/    конвертер ебилдов в .flx
├── recipes/       дерево пакетов: base, core, libs, net, tools
├── profiles/      профили x86-64 и x86-64-musl
├── etc/           os-release и пример flexura.conf
├── fastfetch/     конфиг с логотипом
├── neofetch/      то же для neofetch
├── scripts/       mkstage.sh, install.sh, mkiso.sh и генератор тестового пакета
├── .github/       CI и release-конвейеры
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

- бинарный хост: формат .ftar с метаданными и проверкой сумм
- sandbox для фаз сборки, хуки до и после
- рецепт glibc, чтобы stage перестал зависеть от тулчейна хоста
- sub-слоты и слоты сборщика

## Лицензия

GPL-3.0, полный текст в [LICENSE](LICENSE). Автор mel0k1.
