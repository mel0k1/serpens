use crate::doc::{strip_comment, unquote};

pub fn default_for(phase: &str) -> String {
    match phase {
        "configure" => "./configure --prefix=/usr".to_string(),
        "compile" => "make -j${NPROC}".to_string(),
        "install" => "make DESTDIR=\"${D}\" install".to_string(),
        _ => String::new(),
    }
}

pub fn translate_phase(body: &[String], phase: &str, warnings: &mut Vec<String>) -> Option<String> {
    let mut mapped: Vec<String> = Vec::new();
    let mut into: Option<String> = None;
    for raw in body {
        let line = strip_comment(raw).trim().to_string();
        let line = line.as_str();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("insinto ") {
            into = Some(
                unquote(line.strip_prefix("insinto ").unwrap())
                    .trim_matches('"')
                    .to_string(),
            );
            continue;
        }
        match map_line(line, phase, into.as_deref(), warnings) {
            Mapped::Drop => continue,
            Mapped::Keep(s) => mapped.push(s),
        }
    }
    if mapped.is_empty() {
        if body.is_empty() {
            return None;
        }
        return Some(default_for(phase));
    }
    Some(mapped.join("\n"))
}

enum Mapped {
    Drop,
    Keep(String),
}

fn map_line(line: &str, phase: &str, into: Option<&str>, warnings: &mut Vec<String>) -> Mapped {
    let l = strip_die(line);
    if l.trim().is_empty() {
        return Mapped::Drop;
    }
    let t = l.trim();
    let t = strip_subshell_die(t);
    if t.is_empty() {
        return Mapped::Drop;
    }
    let head = t.split_whitespace().next().unwrap_or("");
    match head {
        "default"
        | "default_src_prepare"
        | "default_src_configure"
        | "default_src_compile"
        | "default_src_install"
        | "default_src_test" => {
            let d = default_for(phase);
            if d.is_empty() {
                return Mapped::Drop;
            }
            return Mapped::Keep(d);
        }
        "econf" => {
            let args = t.strip_prefix("econf").unwrap().trim();
            if args.is_empty() {
                return Mapped::Keep("./configure --prefix=/usr".to_string());
            }
            return Mapped::Keep(format!("./configure --prefix=/usr {}", args));
        }
        "emake" => {
            let args = t.strip_prefix("emake").unwrap().trim();
            return Mapped::Keep(with_jobs("make", args));
        }
        "einstall" => {
            return Mapped::Keep("make DESTDIR=\"${D}\" install".to_string());
        }
        "dodir" => {
            let args = t.strip_prefix("dodir").unwrap().trim();
            return Mapped::Keep(format!("mkdir -p {}", under_d(args)));
        }
        "keepdir" => {
            let args = t.strip_prefix("keepdir").unwrap().trim();
            return Mapped::Keep(format!("mkdir -p {}", under_d(args)));
        }
        "dobin" => {
            let args = t.strip_prefix("dobin").unwrap().trim();
            return Mapped::Keep(loop_install(args, "755", "${D}/usr/bin"));
        }
        "dosbin" => {
            let args = t.strip_prefix("dosbin").unwrap().trim();
            return Mapped::Keep(loop_install(args, "755", "${D}/usr/sbin"));
        }
        "dolib" | "dolib.so" | "dolib.a" => {
            let args = args_of(&t);
            return Mapped::Keep(loop_install(&args, "755", "${D}/usr/lib"));
        }
        "doheader" => {
            let args = t.strip_prefix("doheader").unwrap().trim();
            return Mapped::Keep(loop_install(args, "644", "${D}/usr/include"));
        }
        "dodoc" => {
            let args = t.strip_prefix("dodoc").unwrap().trim();
            return Mapped::Keep(loop_install(args, "644", "${D}/usr/share/doc/${PF}"));
        }
        "doman" | "doman6" | "doman1" => {
            let args = args_of(&t);
            return Mapped::Keep(loop_man(&args));
        }
        "doinfo" => {
            let args = args_of(&t);
            return Mapped::Keep(loop_install(&args, "644", "${D}/usr/share/info"));
        }
        "doins" => {
            let args = args_of(&t);
            let dir = into.unwrap_or("").trim_matches('"').to_string();
            if dir.is_empty() {
                warnings.push(format!(
                    "doins без insinto, строка оставлена как есть: {}",
                    t
                ));
                return Mapped::Keep(t.to_string());
            }
            return Mapped::Keep(loop_install(
                &args,
                "644",
                &format!("${{D}}/{}", dir.trim_start_matches('/')),
            ));
        }
        "newbin" => return Mapped::Keep(new_install(&args_of(&t), "755", "${D}/usr/bin")),
        "newsbin" => return Mapped::Keep(new_install(&args_of(&t), "755", "${D}/usr/sbin")),
        "newconfd" => return Mapped::Keep(new_install(&args_of(&t), "644", "${D}/etc/conf.d")),
        "newinitd" => return Mapped::Keep(new_install(&args_of(&t), "755", "${D}/etc/init.d")),
        "newlib.so" | "newlib.a" => {
            return Mapped::Keep(new_install(&args_of(&t), "755", "${D}/usr/lib"))
        }
        "dosym" => return Mapped::Keep(dosym(&args_of(&t))),
        "cmake_src_configure" => {
            return Mapped::Keep(
                "cmake -B build -S . -DCMAKE_INSTALL_PREFIX=/usr -DCMAKE_BUILD_TYPE=Release"
                    .to_string(),
            )
        }
        "cmake_src_compile" | "cmake_build" => {
            return Mapped::Keep("cmake --build build --parallel ${NPROC}".to_string())
        }
        "cmake_src_install" | "cmake_install" => {
            return Mapped::Keep("DESTDIR=\"${D}\" cmake --install build".to_string())
        }
        "meson_src_configure" => {
            return Mapped::Keep("meson setup build --prefix=/usr".to_string())
        }
        "meson_src_compile" | "meson_compile" => {
            return Mapped::Keep("meson compile -C build -j ${NPROC}".to_string())
        }
        "meson_src_install" | "meson_install" => {
            return Mapped::Keep("DESTDIR=\"${D}\" meson install -C build".to_string())
        }
        "die" => return Mapped::Keep("exit 1".to_string()),
        "python_foo" | "python_domodule" | "python_doscript" | "python_setup" | "python_any_of" => {
            warnings.push(format!("python-команда без перевода: {}", t));
            return Mapped::Keep(t.to_string());
        }
        _ => {}
    }
    if t.contains("$(use_") || t.contains("${use_") {
        return Mapped::Keep(t.to_string());
    }
    Mapped::Keep(t.to_string())
}

fn args_of(t: &str) -> String {
    match t.find(char::is_whitespace) {
        Some(i) => t[i..].trim().to_string(),
        None => String::new(),
    }
}

fn strip_die(line: &str) -> String {
    if let Some(i) = line.find("|| die") {
        let before = &line[..i];
        if before.matches('"').count().is_multiple_of(2) {
            return before.trim_end().to_string();
        }
    }
    line.trim_end().to_string()
}

fn strip_subshell_die(t: &str) -> String {
    let t = t.trim();
    if t.starts_with("die") {
        return String::new();
    }
    t.to_string()
}

fn with_jobs(cmd: &str, args: &str) -> String {
    if args
        .split_whitespace()
        .any(|a| a == "-j" || a.starts_with("-j") || a.starts_with("--jobs"))
    {
        if args.is_empty() {
            cmd.to_string()
        } else {
            format!("{} {}", cmd, args)
        }
    } else if args.is_empty() {
        format!("{} -j${{NPROC}}", cmd)
    } else {
        format!("{} -j${{NPROC}} {}", cmd, args)
    }
}

fn under_d(args: &str) -> String {
    args.split_whitespace()
        .map(|a| {
            if a.contains("${D") || a.contains("$D") {
                a.to_string()
            } else {
                format!("\"${{D}}/{}\"", a.trim_start_matches('/').trim_matches('"'))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn loop_install(args: &str, mode: &str, dest: &str) -> String {
    format!(
        "for _f in {}; do install -Dm{} \"$_f\" \"{}/$(basename \"$_f\")\"; done",
        args, mode, dest
    )
}

fn loop_man(args: &str) -> String {
    format!(
        "for _f in {}; do _s=$(basename \"$_f\" | sed 's/.*\\.//'); install -Dm644 \"$_f\" \"${{D}}/usr/share/man/man$_s/$(basename \"$_f\")\"; done",
        args
    )
}

fn new_install(args: &str, mode: &str, dest: &str) -> String {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() < 2 {
        return format!("install -Dm{} {} \"{}/\"", mode, args, dest);
    }
    let dst = parts[1].trim_matches('"');
    format!("install -Dm{} {} \"{}/{}\"", mode, parts[0], dest, dst)
}

fn dosym(args: &str) -> String {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() < 2 {
        return format!("ln -s {}", args);
    }
    let target = parts[0];
    let link = parts[1].trim_matches('"').trim_start_matches('/');
    format!(
        "mkdir -p \"$(dirname \"${{D}}/{}\")\" && ln -sfn {} \"${{D}}/{}\"",
        link, target, link
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn econf_and_emake() {
        let mut w = Vec::new();
        let body = vec!["econf --with-x".to_string(), "emake".to_string()];
        let s = translate_phase(&body, "configure", &mut w).unwrap();
        assert!(s.contains("./configure --prefix=/usr --with-x"));
        let body = vec![
            "emake".to_string(),
            "emake DESTDIR=\"${D}\" install".to_string(),
        ];
        let s = translate_phase(&body, "compile", &mut w).unwrap();
        assert!(s.contains("make -j${NPROC}"));
        assert!(s.contains("make -j${NPROC} DESTDIR=\"${D}\" install"));
    }

    #[test]
    fn die_is_stripped() {
        let mut w = Vec::new();
        let body = vec!["./configure || die \"configure failed\"".to_string()];
        let s = translate_phase(&body, "configure", &mut w).unwrap();
        assert_eq!(s, "./configure");
    }

    #[test]
    fn install_helpers() {
        let mut w = Vec::new();
        let body = vec![
            "dodir /etc/conf.d".to_string(),
            "dobin bin/tool".to_string(),
            "newbin tool tool-bin".to_string(),
            "dosym tool /usr/bin/link".to_string(),
            "doman man/page.1".to_string(),
            "dodoc README".to_string(),
        ];
        let s = translate_phase(&body, "install", &mut w).unwrap();
        assert!(s.contains("mkdir -p \"${D}/etc/conf.d\""));
        assert!(s.contains("install -Dm755 \"$_f\" \"${D}/usr/bin/$(basename \"$_f\")\""));
        assert!(s.contains("install -Dm755 tool \"${D}/usr/bin/tool-bin\""));
        assert!(s.contains("ln -sfn tool \"${D}/usr/bin/link\""));
        assert!(s.contains("usr/share/man/man$_s/$(basename \"$_f\")"));
        assert!(s.contains("usr/share/doc/${PF}"));
    }

    #[test]
    fn insinto_doins_pair() {
        let mut w = Vec::new();
        let body = vec![
            "insinto /etc/app".to_string(),
            "doins app.conf extra.conf".to_string(),
        ];
        let s = translate_phase(&body, "install", &mut w).unwrap();
        assert!(s.contains("install -Dm644 \"$_f\" \"${D}/etc/app/$(basename \"$_f\")\""));
    }

    #[test]
    fn default_translations() {
        let mut w = Vec::new();
        let body = vec!["default".to_string()];
        assert_eq!(
            translate_phase(&body, "configure", &mut w).unwrap(),
            "./configure --prefix=/usr"
        );
        assert_eq!(
            translate_phase(&body, "compile", &mut w).unwrap(),
            "make -j${NPROC}"
        );
        assert_eq!(
            translate_phase(&body, "install", &mut w).unwrap(),
            "make DESTDIR=\"${D}\" install"
        );
    }

    #[test]
    fn cmake_and_meson() {
        let mut w = Vec::new();
        let body = vec![
            "cmake_src_configure".to_string(),
            "cmake_src_compile".to_string(),
            "cmake_src_install".to_string(),
        ];
        let s = translate_phase(&body, "configure", &mut w).unwrap();
        assert!(s.contains("cmake -B build -S . -DCMAKE_INSTALL_PREFIX=/usr"));
        let body = vec!["meson_src_configure".to_string()];
        let s = translate_phase(&body, "configure", &mut w).unwrap();
        assert!(s.contains("meson setup build --prefix=/usr"));
    }
}
