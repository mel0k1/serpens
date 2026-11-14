use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "flexura",
    version,
    about = "пакетный менеджер дистрибутива Serpens"
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,

    #[arg(
        long,
        global = true,
        value_name = "DIR",
        env = "FLEXURA_ROOT",
        help = "корень файловой системы"
    )]
    pub root: Option<String>,

    #[arg(
        short = 'j',
        long,
        global = true,
        value_name = "N",
        help = "число параллельных задач сборки"
    )]
    pub jobs: Option<usize>,

    #[arg(short, long, global = true, help = "меньше болтовни")]
    pub quiet: bool,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(alias = "update", about = "обновить репозитории рецептов")]
    Sync,

    #[command(aliases = ["emerge", "add"], about = "собрать и установить пакеты")]
    Install {
        #[arg(required = true, value_name = "PKG")]
        packages: Vec<String>,
        #[arg(short, long, help = "пересобрать даже установленную версию")]
        force: bool,
        #[arg(short, long, help = "показать план без сборки")]
        pretend: bool,
        #[arg(short = 'k', long, help = "ставить из тарбола pkgdir, если есть")]
        usepkg: bool,
    },

    #[command(aliases = ["unmerge", "del"], about = "удалить пакеты")]
    Remove {
        #[arg(required = true, value_name = "PKG")]
        packages: Vec<String>,
        #[arg(short = 'y', long, help = "не спрашивать подтверждения")]
        yes: bool,
        #[arg(short, long, help = "показать план без удаления")]
        pretend: bool,
        #[arg(long, help = "удалять даже если пакет кому-то нужен")]
        force: bool,
    },

    #[command(alias = "find", about = "поиск по именам и описаниям")]
    Search {
        #[arg(value_name = "QUERY")]
        query: String,
    },

    #[command(alias = "show", about = "сводка о системе или пакете")]
    Info {
        #[arg(value_name = "PKG")]
        package: Option<String>,
    },

    #[command(alias = "ls", about = "список пакетов")]
    List {
        #[arg(short = 'i', long, help = "только установленные")]
        installed: bool,
    },

    #[command(alias = "tree", about = "дерево зависимостей")]
    Deps {
        #[arg(value_name = "PKG")]
        package: String,
        #[arg(short = 'R', long, help = "кто зависит от пакета")]
        reverse: bool,
    },

    #[command(about = "обновить пакеты из world")]
    Upgrade,

    #[command(alias = "clean", about = "убрать осиротевшие зависимости")]
    Depclean {
        #[arg(short = 'y', long, help = "не спрашивать подтверждения")]
        yes: bool,
        #[arg(short, long, help = "показать кандидатов без удаления")]
        pretend: bool,
    },

    #[command(alias = "pkg", about = "запаковать установленный пакет в тарбол")]
    Quickpkg {
        #[arg(required = true, value_name = "PKG")]
        packages: Vec<String>,
    },

    #[command(alias = "owner", about = "какой пакет владеет файлом")]
    File {
        #[arg(value_name = "PATH")]
        path: String,
    },

    #[command(alias = "download", about = "только скачать исходники")]
    Fetch {
        #[arg(required = true, value_name = "PKG")]
        packages: Vec<String>,
        #[arg(short, long, help = "перекачать даже установленное")]
        force: bool,
    },

    #[command(about = "показать world")]
    World,

    #[command(alias = "qcheck", about = "проверить файлы по sha256 из базы")]
    Verify,

    #[command(about = "история установок")]
    Log {
        #[arg(short = 'n', long, default_value_t = 20, value_name = "N")]
        limit: usize,
    },
}
