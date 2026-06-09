//! The module flattener (v0.126).
//!
//! Resolves a root `.ks` file plus every file it reaches through
//! `@import("…")` into ONE flat [`ast::Module`]: it lexes/parses each file,
//! concatenates all their items (imports erased), and hands the result to the
//! existing `sema`/`emit_c` unchanged.
//!
//! v0.126 is deliberately simple — a `#include`-style flatten:
//! - Import paths are resolved **relative to the importing file's directory**.
//! - The import graph is walked once per file (a file imported twice is only
//!   included once); an import **cycle** is an error.
//! - All top-level item names must be **globally unique** across the program
//!   (a collision is an error). `pub` is not yet enforced across modules and
//!   there is no `m.member` qualified access — both are honest deferrals.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::{Item, Module};
use crate::diag::Diagnostic;
use crate::span::Span;

/// Resolve `root` and its transitive `@import`s into one flattened module.
///
/// Loads `root`, then walks every `@import("path")` it (transitively) reaches,
/// resolving each path relative to the importing file's directory. Every file's
/// non-import items are concatenated into one [`Module`] (imports erased), in a
/// deterministic depth-first order — a file's imported items precede its own —
/// so that a file may reference declarations from the files it imports.
///
/// Errors (all carrying the offending path in their message):
/// - `E0291` — a missing / unreadable import file,
/// - `E0292` — an import cycle,
/// - `E0293` — two top-level items sharing a name,
/// - `E0294` — a lex/parse error inside an imported file, pre-rendered against
///   that file's own source (the flattener owns each file's text; the caller
///   only has the root source, so it cannot render sub-file errors itself).
pub fn resolve(root: &Path) -> Result<Module, Vec<Diagnostic>> {
    // The flat accumulator and a parallel record of which file each item came
    // from (used to name both sides of a duplicate-name collision).
    let mut items: Vec<Item> = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    // Canonical paths already fully included (for dedup of diamond imports).
    let mut visited: HashSet<PathBuf> = HashSet::new();
    // Canonical paths currently being resolved (for cycle detection).
    let mut stack: Vec<PathBuf> = Vec::new();

    resolve_file(
        root,
        Span::DUMMY,
        &mut items,
        &mut files,
        &mut visited,
        &mut stack,
    )?;

    check_unique(&items, &files)?;
    Ok(Module { items })
}

/// Recursively load one file and append its (transitively imported) items.
///
/// `import_span` anchors structural diagnostics about *this* file to the
/// `@import` that referenced it (or [`Span::DUMMY`] for the root). `items` and
/// `files` grow in lock-step: `files[i]` is the canonical source path of
/// `items[i]`.
fn resolve_file(
    path: &Path,
    import_span: Span,
    items: &mut Vec<Item>,
    files: &mut Vec<PathBuf>,
    visited: &mut HashSet<PathBuf>,
    stack: &mut Vec<PathBuf>,
) -> Result<(), Vec<Diagnostic>> {
    // Canonicalise for dedup + cycle detection. A path that cannot be
    // canonicalised does not name a readable file → E0291.
    let canon = match path.canonicalize() {
        Ok(c) => c,
        Err(_) => {
            return Err(vec![Diagnostic::error(
                import_span,
                "E0291",
                format!("cannot find import file `{}`", path.display()),
            )]);
        }
    };

    // A path currently on the resolution stack is being imported again → cycle.
    // (Checked before the visited-dedup so a true cycle is never silently
    // swallowed as a "already included" no-op.)
    if stack.iter().any(|p| p == &canon) {
        return Err(vec![Diagnostic::error(
            import_span,
            "E0292",
            format!(
                "import cycle detected: `{}` imports itself (directly or transitively)",
                canon.display()
            ),
        )]);
    }

    // Already fully included elsewhere (e.g. the shared base of a diamond):
    // include its items only once.
    if visited.contains(&canon) {
        return Ok(());
    }
    visited.insert(canon.clone());

    let src = match std::fs::read_to_string(&canon) {
        Ok(s) => s,
        Err(e) => {
            return Err(vec![Diagnostic::error(
                import_span,
                "E0291",
                format!("cannot read import file `{}`: {}", canon.display(), e),
            )]);
        }
    };

    // Lex + parse against this file's own source. Sub-file diagnostics are
    // rendered here (the flattener owns the text) and bundled into one E0294.
    let tokens = match crate::lexer::lex(&src) {
        Ok(t) => t,
        Err(diags) => return Err(vec![sub_file_error(&diags, &canon, &src)]),
    };
    let module = match crate::parser::parse(&tokens) {
        Ok(m) => m,
        Err(diags) => return Err(vec![sub_file_error(&diags, &canon, &src)]),
    };

    // Mark in-progress and resolve children, then append our own items. The
    // importing file's directory anchors relative import paths.
    stack.push(canon.clone());
    let parent = canon
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // Pass 1 — depth-first: pull in imported items first so this file may refer
    // to declarations from the files it imports.
    for item in &module.items {
        if let Item::Import(imp) = item {
            let target = parent.join(&imp.path);
            resolve_file(&target, imp.span, items, files, visited, stack)?;
        }
    }
    // Pass 2 — append this file's own (non-import) items, erasing the imports.
    for item in module.items {
        if !matches!(item, Item::Import(_)) {
            files.push(canon.clone());
            items.push(item);
        }
    }

    stack.pop();
    Ok(())
}

/// Build the single `E0294` diagnostic that carries a sub-file's pre-rendered
/// lex/parse errors, prefixed with the file path.
fn sub_file_error(diags: &[Diagnostic], path: &Path, src: &str) -> Diagnostic {
    let filename = path.display().to_string();
    let rendered = crate::diag::render_all(diags, &filename, src);
    Diagnostic::error(
        Span::DUMMY,
        "E0294",
        format!("error in imported file `{}`:\n{}", filename, rendered),
    )
}

/// The globally-significant name of a top-level item, or `None` for items with
/// no shared name (tests carry a free-form string label; imports are erased).
fn top_level_name(item: &Item) -> Option<(&str, Span)> {
    match item {
        Item::Func(f) => Some((&f.name, f.span)),
        Item::Const(c) => Some((&c.name, c.span)),
        Item::Struct(s) => Some((&s.name, s.span)),
        Item::Enum(e) => Some((&e.name, e.span)),
        Item::Union(u) => Some((&u.name, u.span)),
        Item::ErrorSet(e) => Some((&e.name, e.span)),
        Item::Test(_) | Item::Import(_) => None,
    }
}

/// Every top-level item name must be unique across the whole flattened program.
/// Reports `E0293` for each collision, naming the duplicate and both files.
fn check_unique(items: &[Item], files: &[PathBuf]) -> Result<(), Vec<Diagnostic>> {
    let mut first_seen: HashMap<&str, usize> = HashMap::new();
    let mut diags: Vec<Diagnostic> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if let Some((name, span)) = top_level_name(item) {
            if let Some(&prev) = first_seen.get(name) {
                diags.push(Diagnostic::error(
                    span,
                    "E0293",
                    format!(
                        "duplicate top-level name `{}` (defined in `{}` and `{}`)",
                        name,
                        files[prev].display(),
                        files[i].display()
                    ),
                ));
            } else {
                first_seen.insert(name, i);
            }
        }
    }
    if diags.is_empty() {
        Ok(())
    } else {
        Err(diags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Create a fresh, uniquely-named temporary directory for a test's files.
    fn fresh_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "kardc_modtest_{}_{}_{}_{}",
            tag,
            std::process::id(),
            nanos,
            n
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write(dir: &Path, name: &str, src: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, src).expect("write temp file");
        p
    }

    /// The globally-named top-level items of a flattened module, in order.
    fn names(m: &Module) -> Vec<String> {
        m.items
            .iter()
            .filter_map(|it| top_level_name(it).map(|(n, _)| n.to_string()))
            .collect()
    }

    fn has_code(diags: &[Diagnostic], code: &str) -> bool {
        diags.iter().any(|d| d.code == code)
    }

    #[test]
    fn flattens_root_and_imported_file() {
        let dir = fresh_dir("flatten");
        write(&dir, "util.ks", "pub fn helper() i32 { return 42; }\n");
        let main = write(
            &dir,
            "main.ks",
            "@import(\"util.ks\");\npub fn main() i32 { return helper(); }\n",
        );

        let result = resolve(&main);
        std::fs::remove_dir_all(&dir).ok();

        let module = result.expect("resolve should succeed");
        let ns = names(&module);
        // Imported items precede the importer's own (depth-first order).
        assert_eq!(ns, vec!["helper".to_string(), "main".to_string()]);
        // Imports are erased from the flat module.
        assert!(
            !module.items.iter().any(|it| matches!(it, Item::Import(_))),
            "flattened module must not contain any Import items"
        );
    }

    #[test]
    fn missing_import_is_e0291() {
        let dir = fresh_dir("missing");
        let main = write(
            &dir,
            "main.ks",
            "@import(\"does_not_exist.ks\");\nfn main() void { }\n",
        );

        let result = resolve(&main);
        std::fs::remove_dir_all(&dir).ok();

        let diags = result.expect_err("missing import must fail");
        assert!(has_code(&diags, "E0291"), "expected E0291, got {:?}", diags);
        assert!(
            diags[0].message.contains("does_not_exist.ks"),
            "E0291 message must name the missing path: {}",
            diags[0].message
        );
    }

    #[test]
    fn import_cycle_is_e0292() {
        let dir = fresh_dir("cycle");
        let a = write(&dir, "a.ks", "@import(\"b.ks\");\nfn fa() void { }\n");
        write(&dir, "b.ks", "@import(\"a.ks\");\nfn fb() void { }\n");

        let result = resolve(&a);
        std::fs::remove_dir_all(&dir).ok();

        let diags = result.expect_err("a 2-file cycle must fail");
        assert!(has_code(&diags, "E0292"), "expected E0292, got {:?}", diags);
    }

    #[test]
    fn duplicate_top_level_name_is_e0293() {
        let dir = fresh_dir("dup");
        write(&dir, "two.ks", "fn shared() void { }\n");
        let one = write(
            &dir,
            "one.ks",
            "@import(\"two.ks\");\nfn shared() void { }\n",
        );

        let result = resolve(&one);
        std::fs::remove_dir_all(&dir).ok();

        let diags = result.expect_err("duplicate fn name must fail");
        assert!(has_code(&diags, "E0293"), "expected E0293, got {:?}", diags);
        assert!(
            diags[0].message.contains("shared"),
            "E0293 message must name the duplicate: {}",
            diags[0].message
        );
    }

    #[test]
    fn diamond_includes_shared_base_once() {
        let dir = fresh_dir("diamond");
        write(&dir, "d.ks", "fn fd() i32 { return 4; }\n");
        write(&dir, "b.ks", "@import(\"d.ks\");\nfn fb() i32 { return fd(); }\n");
        write(&dir, "c.ks", "@import(\"d.ks\");\nfn fc() i32 { return fd(); }\n");
        let a = write(
            &dir,
            "a.ks",
            "@import(\"b.ks\");\n@import(\"c.ks\");\nfn fa() i32 { return fb() + fc(); }\n",
        );

        let result = resolve(&a);
        std::fs::remove_dir_all(&dir).ok();

        let module = result.expect("diamond must resolve without error");
        let ns = names(&module);
        // `fd` is reachable through both `b` and `c` but must appear once.
        let fd_count = ns.iter().filter(|n| n.as_str() == "fd").count();
        assert_eq!(fd_count, 1, "shared base included {} times: {:?}", fd_count, ns);
        // All four functions are present, none duplicated → no E0293.
        let mut sorted = ns.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            vec![
                "fa".to_string(),
                "fb".to_string(),
                "fc".to_string(),
                "fd".to_string()
            ]
        );
    }

    #[test]
    fn sub_file_parse_error_is_e0294() {
        let dir = fresh_dir("parseerr");
        // A syntactically broken import target.
        write(&dir, "bad.ks", "fn oops( {\n");
        let main = write(&dir, "main.ks", "@import(\"bad.ks\");\nfn main() void { }\n");

        let result = resolve(&main);
        std::fs::remove_dir_all(&dir).ok();

        let diags = result.expect_err("a broken imported file must fail");
        assert!(has_code(&diags, "E0294"), "expected E0294, got {:?}", diags);
        assert!(
            diags[0].message.contains("bad.ks"),
            "E0294 message must name the offending file: {}",
            diags[0].message
        );
    }

    #[test]
    fn missing_root_file_is_e0291() {
        let dir = fresh_dir("noroot");
        let root = dir.join("nope.ks");
        let result = resolve(&root);
        std::fs::remove_dir_all(&dir).ok();
        let diags = result.expect_err("a missing root must fail");
        assert!(has_code(&diags, "E0291"), "expected E0291, got {:?}", diags);
    }
}
