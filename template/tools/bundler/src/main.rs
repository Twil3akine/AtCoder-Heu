use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use syn::visit::{self, Visit};
use syn::visit_mut::{self, VisitMut};
use syn::{Attribute, File, Item, ItemMod, Path as SynPath, UseTree};

type ModulePath = Vec<String>;

#[derive(Default)]
struct Selection {
    modules: BTreeSet<ModulePath>,
    items: BTreeMap<ModulePath, Option<BTreeSet<String>>>,
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
struct Dependency {
    module: ModulePath,
    item: Option<String>,
}

#[derive(Clone)]
struct ModuleInfo {
    file: PathBuf,
    syntax: File,
    declaration: ItemMod,
}

struct Library {
    root: File,
    modules: BTreeMap<ModulePath, ModuleInfo>,
    exported_macros: BTreeMap<String, ModulePath>,
    root_reexports: BTreeMap<String, Dependency>,
}

struct Options {
    copy_to_clipboard: bool,
    output: Option<PathBuf>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let options = parse_options()?;
    let project = find_project_root(&std::env::current_dir()?)?;
    let manifest = read_manifest(&project.join("Cargo.toml"))?;
    let crate_name = manifest
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .context("Cargo.toml に package.name がありません")?
        .replace('-', "_");
    let edition = manifest
        .get("package")
        .and_then(|package| package.get("edition"))
        .and_then(toml::Value::as_str)
        .unwrap_or("2021");

    let main_file = project.join("src/main.rs");
    let library_file = project.join("src/lib.rs");
    let main_source = fs::read_to_string(&main_file)
        .with_context(|| format!("{} を読み取れません", main_file.display()))?;
    let mut main_syntax =
        syn::parse_file(&main_source).context("src/main.rs の構文解析に失敗しました")?;
    let library = Library::load(&library_file)?;
    let root_reexports = used_root_reexports(&main_syntax, &library, &crate_name);

    let selected = select_library(&main_syntax, &library, &crate_name);
    let mut bundled_modules = Vec::new();
    for item in &library
        .root
        .items
    {
        let Item::Mod(declaration) = item else {
            continue;
        };
        let path = vec![declaration
            .ident
            .to_string()];
        if selected
            .modules
            .contains(&path)
        {
            bundled_modules.push(inline_module(&path, declaration, &library, &selected)?);
        }
    }

    remove_root_macro_imports(
        &mut main_syntax.items,
        &crate_name,
        &library.exported_macros,
    );
    remove_root_reexport_imports(&mut main_syntax.items, &crate_name, &root_reexports);
    CratePathRewriter {
        crate_name: &crate_name,
    }
    .visit_file_mut(&mut main_syntax);
    prune_tests(&mut main_syntax.items);
    strip_docs_from_items(&mut main_syntax.items);
    strip_doc_attributes(&mut main_syntax.attrs);

    let insertion = main_syntax
        .items
        .iter()
        .position(|item| !matches!(item, Item::ExternCrate(_) | Item::Use(_)))
        .unwrap_or(
            main_syntax
                .items
                .len(),
        );
    let mut bundled_items = root_reexport_items(&root_reexports, &library)?;
    bundled_items.extend(
        bundled_modules
            .into_iter()
            .map(Item::Mod),
    );
    main_syntax
        .items
        .splice(insertion..insertion, bundled_items);

    let bundled = prettyplease::unparse(&main_syntax);
    let output = options
        .output
        .map(|path| project.join(path))
        .unwrap_or_else(|| project.join("target/bundle.rs"));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, &bundled)
        .with_context(|| format!("{} を書き込めません", output.display()))?;

    compile_with_cargo(&output, edition, &manifest, &project)?;
    if options.copy_to_clipboard {
        copy_to_clipboard(&bundled)?;
    }

    let reported = selected
        .modules
        .iter()
        .filter(|path| module_has_code(&library.modules[*path].syntax))
        .map(|path| path.join("::"))
        .collect::<Vec<_>>();
    let source_size = main_source.len()
        + selected
            .modules
            .iter()
            .map(|path| fs::metadata(&library.modules[path].file).map(|meta| meta.len() as usize))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .sum::<usize>();

    println!("Bundled {} modules:", reported.len());
    for module in reported {
        println!("{module}");
    }
    println!();
    println!(
        "Bundle size: {} -> {}",
        format_size(source_size),
        format_size(bundled.len())
    );
    println!();
    println!("✓ cargo check succeeded");
    if options.copy_to_clipboard {
        println!("✓ copied to clipboard");
    }
    println!(
        "✓ wrote {}",
        output
            .strip_prefix(&project)
            .unwrap_or(&output)
            .display()
    );
    Ok(())
}

fn parse_options() -> Result<Options> {
    let mut options = Options {
        copy_to_clipboard: true,
        output: None,
    };
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--no-clipboard" => options.copy_to_clipboard = false,
            "--output" => {
                options.output = Some(PathBuf::from(
                    arguments
                        .next()
                        .context("--output にはパスが必要です")?,
                ));
            }
            "-h" | "--help" => {
                println!("Usage: cargo bundle [--no-clipboard] [--output PATH]");
                std::process::exit(0);
            }
            _ => bail!("不明な引数です: {argument}"),
        }
    }
    Ok(options)
}

fn find_project_root(start: &Path) -> Result<PathBuf> {
    for directory in start.ancestors() {
        if directory
            .join("Cargo.toml")
            .is_file()
            && directory
                .join("src/main.rs")
                .is_file()
            && directory
                .join("src/lib.rs")
                .is_file()
        {
            return Ok(directory.to_path_buf());
        }
    }
    bail!("src/main.rs と src/lib.rs を持つ Cargo project が見つかりません")
}

fn read_manifest(path: &Path) -> Result<toml::Value> {
    let source = fs::read_to_string(path)?;
    toml::from_str(&source).with_context(|| format!("{} を解析できません", path.display()))
}

impl Library {
    fn load(root_file: &Path) -> Result<Self> {
        let source = fs::read_to_string(root_file)?;
        let root = syn::parse_file(&source).context("src/lib.rs の構文解析に失敗しました")?;
        let mut library = Self {
            root: root.clone(),
            modules: BTreeMap::new(),
            exported_macros: BTreeMap::new(),
            root_reexports: BTreeMap::new(),
        };
        library.load_children(&[], root_file, &root.items)?;
        library.load_root_reexports();
        Ok(library)
    }

    fn load_root_reexports(&mut self) {
        for item in &self
            .root
            .items
        {
            let Item::Use(item_use) = item else {
                continue;
            };
            if !matches!(item_use.vis, syn::Visibility::Public(_)) {
                continue;
            }
            let mut paths = Vec::new();
            collect_use_tree(&item_use.tree, &mut Vec::new(), &mut paths);
            for path in paths {
                let Some(module) = longest_module_prefix(&path, &self.modules) else {
                    continue;
                };
                let Some(item) = path
                    .get(module.len())
                    .cloned()
                else {
                    continue;
                };
                let Some(exported_name) = path
                    .last()
                    .cloned()
                else {
                    continue;
                };
                self.root_reexports
                    .insert(
                        exported_name,
                        Dependency {
                            module,
                            item: Some(item),
                        },
                    );
            }
        }
    }

    fn load_children(
        &mut self,
        parent: &[String],
        parent_file: &Path,
        items: &[Item],
    ) -> Result<()> {
        let declarations = items
            .iter()
            .filter_map(|item| match item {
                Item::Mod(module)
                    if module
                        .content
                        .is_none()
                        && !is_test_only(&module.attrs) =>
                {
                    Some(module.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        for declaration in declarations {
            let mut path = parent.to_vec();
            path.push(
                declaration
                    .ident
                    .to_string(),
            );
            let file = resolve_module_file(parent_file, &declaration)?;
            let source = fs::read_to_string(&file)
                .with_context(|| format!("module {} を読み取れません", path.join("::")))?;
            let syntax = syn::parse_file(&source)
                .with_context(|| format!("module {} の構文解析に失敗しました", path.join("::")))?;

            for item in &syntax.items {
                if let Item::Macro(item_macro) = item {
                    if has_attribute(&item_macro.attrs, "macro_export") {
                        if let Some(identifier) = &item_macro.ident {
                            self.exported_macros
                                .insert(identifier.to_string(), path.clone());
                        }
                    }
                }
            }

            self.modules
                .insert(
                    path.clone(),
                    ModuleInfo {
                        file: file.clone(),
                        syntax: syntax.clone(),
                        declaration,
                    },
                );
            self.load_children(&path, &file, &syntax.items)?;
        }
        Ok(())
    }
}

fn resolve_module_file(
    parent_file: &Path,
    module: &ItemMod,
) -> Result<PathBuf> {
    if let Some(path) = module
        .attrs
        .iter()
        .find_map(path_attribute)
    {
        return Ok(parent_file
            .parent()
            .unwrap()
            .join(path));
    }
    let parent = parent_file
        .parent()
        .unwrap();
    let module_directory = match parent_file
        .file_stem()
        .and_then(|name| name.to_str())
    {
        Some("lib" | "main" | "mod") => parent.to_path_buf(),
        Some(stem) => parent.join(stem),
        None => parent.to_path_buf(),
    };
    let name = module
        .ident
        .to_string();
    let file = module_directory.join(format!("{name}.rs"));
    if file.is_file() {
        return Ok(file);
    }
    let mod_file = module_directory
        .join(name)
        .join("mod.rs");
    if mod_file.is_file() {
        return Ok(mod_file);
    }
    bail!("module {} のソースファイルが見つかりません", module.ident)
}

fn path_attribute(attribute: &Attribute) -> Option<PathBuf> {
    if !attribute
        .path()
        .is_ident("path")
    {
        return None;
    }
    let syn::Meta::NameValue(meta) = &attribute.meta else {
        return None;
    };
    let syn::Expr::Lit(expression) = &meta.value else {
        return None;
    };
    let syn::Lit::Str(path) = &expression.lit else {
        return None;
    };
    Some(PathBuf::from(path.value()))
}

fn select_library(
    main: &File,
    library: &Library,
    crate_name: &str,
) -> Selection {
    let mut selected = Selection::default();
    let mut queue = VecDeque::new();
    for dependency in dependencies(main, &[], library, crate_name, true) {
        add_dependency(dependency, &mut selected, &mut queue);
    }

    while let Some(module) = queue.pop_front() {
        let Some(info) = library
            .modules
            .get(&module)
        else {
            continue;
        };
        for dependency in dependencies(&info.syntax, &module, library, crate_name, false) {
            add_dependency(dependency, &mut selected, &mut queue);
        }
    }
    selected
}

fn used_root_reexports(
    main: &File,
    library: &Library,
    crate_name: &str,
) -> BTreeSet<String> {
    let mut collector = PathCollector::default();
    collector.visit_file(main);
    collector
        .paths
        .into_iter()
        .filter_map(|path| {
            if path.len() == 2
                && path[0] == crate_name
                && library
                    .root_reexports
                    .contains_key(&path[1])
            {
                Some(path[1].clone())
            } else {
                None
            }
        })
        .collect()
}

fn root_reexport_items(
    names: &BTreeSet<String>,
    library: &Library,
) -> Result<Vec<Item>> {
    names
        .iter()
        .map(|name| {
            let dependency = &library.root_reexports[name];
            let item = dependency
                .item
                .as_deref()
                .context("root re-exportにitemがありません")?;
            let path = dependency
                .module
                .join("::");
            let item_use = syn::parse_str::<syn::ItemUse>(&format!("use crate::{path}::{item};"))?;
            Ok(Item::Use(item_use))
        })
        .collect()
}

fn add_dependency(
    dependency: Dependency,
    selected: &mut Selection,
    queue: &mut VecDeque<ModulePath>,
) {
    for length in 1..=dependency
        .module
        .len()
    {
        let ancestor = dependency.module[..length].to_vec();
        if selected
            .modules
            .insert(ancestor.clone())
        {
            queue.push_back(ancestor);
        }
    }

    let entry = selected
        .items
        .entry(dependency.module)
        .or_insert_with(|| Some(BTreeSet::new()));
    match (entry, dependency.item) {
        (slot @ Some(_), None) => *slot = None,
        (Some(items), Some(item)) => {
            items.insert(item);
        }
        (None, _) => {}
    }
}

fn dependencies(
    syntax: &File,
    current: &[String],
    library: &Library,
    crate_name: &str,
    is_main: bool,
) -> BTreeSet<Dependency> {
    let mut syntax = syntax.clone();
    prune_tests(&mut syntax.items);
    let mut collector = PathCollector::default();
    collector.visit_file(&syntax);
    let mut result = BTreeSet::new();

    for raw in collector.paths {
        if raw.is_empty() {
            continue;
        }
        if !is_main && raw.len() == 1 && raw[0] == "self" {
            continue;
        }
        let candidate = if is_main {
            if raw[0] != crate_name {
                continue;
            }
            raw[1..].to_vec()
        } else if raw[0] == "crate" {
            raw[1..].to_vec()
        } else if raw[0] == "self" {
            current
                .iter()
                .cloned()
                .chain(
                    raw[1..]
                        .iter()
                        .cloned(),
                )
                .collect()
        } else if raw[0] == "super" {
            let count = raw
                .iter()
                .take_while(|part| part.as_str() == "super")
                .count();
            let keep = current
                .len()
                .saturating_sub(count);
            current[..keep]
                .iter()
                .cloned()
                .chain(
                    raw[count..]
                        .iter()
                        .cloned(),
                )
                .collect()
        } else {
            continue;
        };

        if let Some(module) = longest_module_prefix(&candidate, &library.modules) {
            let item = candidate
                .get(module.len())
                .filter(|name| name.as_str() != "self")
                .cloned();
            result.insert(Dependency { module, item });
        } else if is_main {
            if candidate.len() == 1 {
                if let Some(dependency) = library
                    .root_reexports
                    .get(&candidate[0])
                {
                    result.insert(dependency.clone());
                    continue;
                }
            }
            if let Some(name) = candidate.last() {
                if let Some(module) = library
                    .exported_macros
                    .get(name)
                {
                    result.insert(Dependency {
                        module: module.clone(),
                        item: None,
                    });
                }
            }
        }
    }
    result
}

fn longest_module_prefix(
    path: &[String],
    modules: &BTreeMap<ModulePath, ModuleInfo>,
) -> Option<ModulePath> {
    (1..=path.len())
        .rev()
        .map(|length| path[..length].to_vec())
        .find(|prefix| modules.contains_key(prefix))
}

#[derive(Default)]
struct PathCollector {
    paths: Vec<ModulePath>,
}

impl<'ast> Visit<'ast> for PathCollector {
    fn visit_item_use(
        &mut self,
        item: &'ast syn::ItemUse,
    ) {
        collect_use_tree(&item.tree, &mut Vec::new(), &mut self.paths);
    }

    fn visit_path(
        &mut self,
        path: &'ast SynPath,
    ) {
        self.paths
            .push(
                path.segments
                    .iter()
                    .map(|part| {
                        part.ident
                            .to_string()
                    })
                    .collect(),
            );
        visit::visit_path(self, path);
    }
}

fn collect_use_tree(
    tree: &UseTree,
    prefix: &mut ModulePath,
    output: &mut Vec<ModulePath>,
) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(
                path.ident
                    .to_string(),
            );
            collect_use_tree(&path.tree, prefix, output);
            prefix.pop();
        }
        UseTree::Name(name) => {
            prefix.push(
                name.ident
                    .to_string(),
            );
            output.push(prefix.clone());
            prefix.pop();
        }
        UseTree::Rename(rename) => {
            prefix.push(
                rename
                    .ident
                    .to_string(),
            );
            output.push(prefix.clone());
            prefix.pop();
        }
        UseTree::Glob(_) => output.push(prefix.clone()),
        UseTree::Group(group) => {
            for tree in &group.items {
                collect_use_tree(tree, prefix, output);
            }
        }
    }
}

struct CratePathRewriter<'a> {
    crate_name: &'a str,
}

impl VisitMut for CratePathRewriter<'_> {
    fn visit_path_mut(
        &mut self,
        path: &mut SynPath,
    ) {
        if let Some(first) = path
            .segments
            .first_mut()
        {
            if first.ident == self.crate_name {
                first.ident = syn::Ident::new(
                    "crate",
                    first
                        .ident
                        .span(),
                );
            }
        }
        visit_mut::visit_path_mut(self, path);
    }

    fn visit_item_use_mut(
        &mut self,
        item: &mut syn::ItemUse,
    ) {
        rewrite_use_tree(&mut item.tree, self.crate_name);
        visit_mut::visit_item_use_mut(self, item);
    }
}

fn rewrite_use_tree(
    tree: &mut UseTree,
    crate_name: &str,
) {
    if let UseTree::Path(path) = tree {
        if path.ident == crate_name {
            path.ident = syn::Ident::new(
                "crate",
                path.ident
                    .span(),
            );
        }
        rewrite_use_tree(&mut path.tree, crate_name);
    } else if let UseTree::Group(group) = tree {
        for tree in &mut group.items {
            rewrite_use_tree(tree, crate_name);
        }
    }
}

fn remove_root_reexport_imports(
    items: &mut Vec<Item>,
    crate_name: &str,
    names: &BTreeSet<String>,
) {
    items.retain_mut(|item| {
        let Item::Use(item_use) = item else {
            return true;
        };
        let Some(tree) =
            filter_root_reexport_imports(&item_use.tree, &mut Vec::new(), crate_name, names)
        else {
            return false;
        };
        item_use.tree = tree;
        true
    });
}

fn filter_root_reexport_imports(
    tree: &UseTree,
    prefix: &mut ModulePath,
    crate_name: &str,
    names: &BTreeSet<String>,
) -> Option<UseTree> {
    match tree {
        UseTree::Path(path) => {
            prefix.push(
                path.ident
                    .to_string(),
            );
            let child = filter_root_reexport_imports(&path.tree, prefix, crate_name, names);
            prefix.pop();
            child.map(|child| {
                let mut path = path.clone();
                path.tree = Box::new(child);
                UseTree::Path(path)
            })
        }
        UseTree::Name(name) => {
            let is_root_reexport = prefix.len() == 1
                && prefix[0] == crate_name
                && names.contains(
                    &name
                        .ident
                        .to_string(),
                );
            (!is_root_reexport).then(|| tree.clone())
        }
        UseTree::Rename(rename) => {
            let is_root_reexport = prefix.len() == 1
                && prefix[0] == crate_name
                && names.contains(
                    &rename
                        .ident
                        .to_string(),
                );
            (!is_root_reexport).then(|| tree.clone())
        }
        UseTree::Glob(_) => Some(tree.clone()),
        UseTree::Group(group) => {
            let mut group = group.clone();
            group.items = group
                .items
                .iter()
                .filter_map(|tree| filter_root_reexport_imports(tree, prefix, crate_name, names))
                .collect();
            (!group
                .items
                .is_empty())
            .then_some(UseTree::Group(group))
        }
    }
}

fn remove_root_macro_imports(
    items: &mut Vec<Item>,
    crate_name: &str,
    macros: &BTreeMap<String, ModulePath>,
) {
    items.retain_mut(|item| {
        let Item::Use(item_use) = item else {
            return true;
        };
        let Some(tree) = filter_macro_imports(&item_use.tree, &mut Vec::new(), crate_name, macros)
        else {
            return false;
        };
        item_use.tree = tree;
        true
    });
}

fn filter_macro_imports(
    tree: &UseTree,
    prefix: &mut ModulePath,
    crate_name: &str,
    macros: &BTreeMap<String, ModulePath>,
) -> Option<UseTree> {
    match tree {
        UseTree::Path(path) => {
            prefix.push(
                path.ident
                    .to_string(),
            );
            let child = filter_macro_imports(&path.tree, prefix, crate_name, macros);
            prefix.pop();
            child.map(|child| {
                let mut path = path.clone();
                path.tree = Box::new(child);
                UseTree::Path(path)
            })
        }
        UseTree::Name(name) => {
            let is_root_macro = prefix.len() == 1
                && prefix[0] == crate_name
                && macros.contains_key(
                    &name
                        .ident
                        .to_string(),
                );
            (!is_root_macro).then(|| UseTree::Name(name.clone()))
        }
        UseTree::Rename(rename) => {
            let is_root_macro = prefix.len() == 1
                && prefix[0] == crate_name
                && macros.contains_key(
                    &rename
                        .ident
                        .to_string(),
                );
            (!is_root_macro).then(|| UseTree::Rename(rename.clone()))
        }
        UseTree::Glob(glob) => Some(UseTree::Glob(glob.clone())),
        UseTree::Group(group) => {
            let mut group = group.clone();
            group.items = group
                .items
                .iter()
                .filter_map(|tree| filter_macro_imports(tree, prefix, crate_name, macros))
                .collect();
            (!group
                .items
                .is_empty())
            .then_some(UseTree::Group(group))
        }
    }
}

fn inline_module(
    path: &[String],
    declaration: &ItemMod,
    library: &Library,
    selected: &Selection,
) -> Result<ItemMod> {
    let info = library
        .modules
        .get(path)
        .with_context(|| format!("module {} が index にありません", path.join("::")))?;
    let mut items = info
        .syntax
        .items
        .clone();
    if let Some(Some(item_names)) = selected
        .items
        .get(path)
    {
        prune_unselected_items(&mut items, item_names);
    }
    let mut expanded = Vec::new();
    for item in items.drain(..) {
        if let Item::Mod(child) = &item {
            if child
                .content
                .is_none()
            {
                let mut child_path = path.to_vec();
                child_path.push(
                    child
                        .ident
                        .to_string(),
                );
                if selected
                    .modules
                    .contains(&child_path)
                {
                    expanded.push(Item::Mod(inline_module(
                        &child_path,
                        &library.modules[&child_path].declaration,
                        library,
                        selected,
                    )?));
                }
                continue;
            }
        }
        expanded.push(item);
    }
    prune_tests(&mut expanded);
    strip_docs_from_items(&mut expanded);

    let mut module = declaration.clone();
    module
        .attrs
        .retain(|attribute| {
            !attribute
                .path()
                .is_ident("path")
                && !attribute
                    .path()
                    .is_ident("doc")
        });
    module.content = Some((syn::token::Brace::default(), expanded));
    module.semi = None;
    Ok(module)
}

fn prune_unselected_items(
    items: &mut Vec<Item>,
    selected: &BTreeSet<String>,
) {
    let named_items = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| item_name(item).map(|name| (name, index)))
        .collect::<BTreeMap<_, _>>();
    let mut retained_names = selected.clone();
    let mut retained = vec![false; items.len()];

    loop {
        let mut changed = false;

        for name in retained_names.clone() {
            if let Some(&index) = named_items.get(&name) {
                if !retained[index] {
                    retained[index] = true;
                    changed = true;
                }
            }
        }

        for (index, item) in items
            .iter()
            .enumerate()
        {
            let keep_unnamed = matches!(item, Item::Use(_) | Item::ExternCrate(_) | Item::Macro(_));
            let keep_impl = match item {
                Item::Impl(item_impl) => impl_names(item_impl)
                    .iter()
                    .any(|name| retained_names.contains(name)),
                _ => false,
            };
            if (keep_unnamed || keep_impl) && !retained[index] {
                retained[index] = true;
                changed = true;
            }
        }

        for (index, item) in items
            .iter()
            .enumerate()
        {
            if !retained[index] {
                continue;
            }
            let mut collector = PathCollector::default();
            collector.visit_item(item);
            for path in collector.paths {
                let Some(name) = path.first() else {
                    continue;
                };
                if named_items.contains_key(name) && retained_names.insert(name.clone()) {
                    changed = true;
                }
            }
        }

        if !changed {
            break;
        }
    }

    let mut index = 0;
    items.retain(|_| {
        let keep = retained[index];
        index += 1;
        keep
    });
}

fn item_name(item: &Item) -> Option<String> {
    let identifier = match item {
        Item::Const(item) => &item.ident,
        Item::Enum(item) => &item.ident,
        Item::Fn(item) => {
            &item
                .sig
                .ident
        }
        Item::Mod(item) => &item.ident,
        Item::Static(item) => &item.ident,
        Item::Struct(item) => &item.ident,
        Item::Trait(item) => &item.ident,
        Item::TraitAlias(item) => &item.ident,
        Item::Type(item) => &item.ident,
        Item::Union(item) => &item.ident,
        _ => return None,
    };
    Some(identifier.to_string())
}

fn impl_names(item: &syn::ItemImpl) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some((_, path, _)) = &item.trait_ {
        if let Some(segment) = path
            .segments
            .last()
        {
            names.insert(
                segment
                    .ident
                    .to_string(),
            );
        }
    }
    let mut collector = PathCollector::default();
    collector.visit_type(&item.self_ty);
    for path in collector.paths {
        if let Some(name) = path.last() {
            names.insert(name.clone());
        }
    }
    names
}

fn prune_tests(items: &mut Vec<Item>) {
    items.retain(|item| !is_test_only(item_attributes(item)));
    for item in items {
        if let Item::Mod(module) = item {
            if let Some((_, items)) = &mut module.content {
                prune_tests(items);
            }
        }
    }
}

fn strip_docs_from_items(items: &mut [Item]) {
    for item in items {
        strip_doc_attributes(item_attributes_mut(item));
        match item {
            Item::Enum(item) => {
                for variant in &mut item.variants {
                    strip_doc_attributes(&mut variant.attrs);
                    for field in &mut variant.fields {
                        strip_doc_attributes(&mut field.attrs);
                    }
                }
            }
            Item::Impl(item) => {
                for member in &mut item.items {
                    let attrs = match member {
                        syn::ImplItem::Const(item) => &mut item.attrs,
                        syn::ImplItem::Fn(item) => &mut item.attrs,
                        syn::ImplItem::Type(item) => &mut item.attrs,
                        syn::ImplItem::Macro(item) => &mut item.attrs,
                        _ => continue,
                    };
                    strip_doc_attributes(attrs);
                }
            }
            Item::Mod(module) => {
                if let Some((_, items)) = &mut module.content {
                    strip_docs_from_items(items);
                }
            }
            Item::Struct(item) => {
                for field in &mut item.fields {
                    strip_doc_attributes(&mut field.attrs);
                }
            }
            Item::Trait(item) => {
                for member in &mut item.items {
                    let attrs = match member {
                        syn::TraitItem::Const(item) => &mut item.attrs,
                        syn::TraitItem::Fn(item) => &mut item.attrs,
                        syn::TraitItem::Type(item) => &mut item.attrs,
                        syn::TraitItem::Macro(item) => &mut item.attrs,
                        _ => continue,
                    };
                    strip_doc_attributes(attrs);
                }
            }
            Item::Union(item) => {
                for field in &mut item
                    .fields
                    .named
                {
                    strip_doc_attributes(&mut field.attrs);
                }
            }
            _ => {}
        }
    }
}

fn item_attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn item_attributes_mut(item: &mut Item) -> &mut Vec<Attribute> {
    match item {
        Item::Const(item) => &mut item.attrs,
        Item::Enum(item) => &mut item.attrs,
        Item::ExternCrate(item) => &mut item.attrs,
        Item::Fn(item) => &mut item.attrs,
        Item::ForeignMod(item) => &mut item.attrs,
        Item::Impl(item) => &mut item.attrs,
        Item::Macro(item) => &mut item.attrs,
        Item::Mod(item) => &mut item.attrs,
        Item::Static(item) => &mut item.attrs,
        Item::Struct(item) => &mut item.attrs,
        Item::Trait(item) => &mut item.attrs,
        Item::TraitAlias(item) => &mut item.attrs,
        Item::Type(item) => &mut item.attrs,
        Item::Union(item) => &mut item.attrs,
        Item::Use(item) => &mut item.attrs,
        _ => panic!("unsupported verbatim item"),
    }
}

fn strip_doc_attributes(attributes: &mut Vec<Attribute>) {
    attributes.retain(|attribute| {
        !attribute
            .path()
            .is_ident("doc")
    });
}

fn is_test_only(attributes: &[Attribute]) -> bool {
    has_attribute(attributes, "test")
        || attributes
            .iter()
            .any(|attribute| {
                if !attribute
                    .path()
                    .is_ident("cfg")
                {
                    return false;
                }
                let syn::Meta::List(list) = &attribute.meta else {
                    return false;
                };
                let mut test = false;
                let _ = list.parse_nested_meta(|meta| {
                    if meta
                        .path
                        .is_ident("test")
                    {
                        test = true;
                    }
                    Ok(())
                });
                test
            })
}

fn has_attribute(
    attributes: &[Attribute],
    name: &str,
) -> bool {
    attributes
        .iter()
        .any(|attribute| {
            attribute
                .path()
                .is_ident(name)
        })
}

fn module_has_code(syntax: &File) -> bool {
    syntax
        .items
        .iter()
        .any(|item| {
            !matches!(item, Item::Mod(module) if module.content.is_none())
                && !is_test_only(item_attributes(item))
        })
}

fn compile_with_cargo(
    source: &Path,
    edition: &str,
    manifest: &toml::Value,
    project: &Path,
) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source_directory = directory
        .path()
        .join("src");
    fs::create_dir(&source_directory)?;
    fs::copy(source, source_directory.join("main.rs"))?;

    let mut package = toml::map::Map::new();
    package.insert(
        "name".into(),
        manifest
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or("bundle-check")
            .into(),
    );
    package.insert(
        "version".into(),
        manifest
            .get("package")
            .and_then(|package| package.get("version"))
            .and_then(toml::Value::as_str)
            .unwrap_or("0.0.0")
            .into(),
    );
    package.insert("edition".into(), edition.into());

    let mut check_manifest = toml::map::Map::new();
    check_manifest.insert("package".into(), toml::Value::Table(package));
    for key in ["dependencies", "features", "target", "patch"] {
        if let Some(value) = manifest.get(key) {
            check_manifest.insert(key.into(), value.clone());
        }
    }
    fs::write(
        directory
            .path()
            .join("Cargo.toml"),
        toml::to_string(&check_manifest)?,
    )?;

    let lock_file = project.join("Cargo.lock");
    if lock_file.is_file() {
        fs::copy(
            &lock_file,
            directory
                .path()
                .join("Cargo.lock"),
        )?;
    }

    let mut command = Command::new("cargo");
    command
        .arg("check")
        .arg("--quiet");
    if lock_file.is_file() {
        command.arg("--locked");
    }
    let output = command
        .arg("--manifest-path")
        .arg(
            directory
                .path()
                .join("Cargo.toml"),
        )
        .arg("--target-dir")
        .arg(project.join("target"))
        .output()
        .context("cargo check を実行できません")?;
    if !output
        .status
        .success()
    {
        bail!(
            "生成コードの cargo check に失敗しました:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn copy_to_clipboard(source: &str) -> Result<()> {
    let mut process = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .context("pbcopy を起動できません")?;
    process
        .stdin
        .take()
        .context("pbcopy の標準入力を開けません")?
        .write_all(source.as_bytes())?;
    let status = process.wait()?;
    if !status.success() {
        bail!("pbcopy が失敗しました");
    }
    Ok(())
}

fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        collect_use_tree, compile_with_cargo, filter_macro_imports, format_size, inline_module,
        item_name, remove_root_reexport_imports, root_reexport_items, select_library,
        used_root_reexports, Item, Library, ModulePath,
    };

    #[test]
    fn flattens_grouped_use_paths() {
        let item: syn::ItemUse = syn::parse_quote!(
            use atcoder::geometry::{Point, ccw};
        );
        let mut paths = Vec::new();
        collect_use_tree(&item.tree, &mut ModulePath::new(), &mut paths);
        assert_eq!(
            paths,
            [
                vec!["atcoder", "geometry", "Point"],
                vec!["atcoder", "geometry", "ccw"]
            ]
        );
    }

    #[test]
    fn formats_kibibytes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1536), "1.5 KiB");
    }

    #[test]
    fn resolves_transitive_module_dependencies() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory
            .path()
            .join("src");
        fs::create_dir_all(source.join("lib")).unwrap();
        fs::write(
            source.join("lib.rs"),
            "#[path = \"lib/a.rs\"] pub mod a;\n#[path = \"lib/b.rs\"] pub mod b;\n",
        )
        .unwrap();
        fs::write(
            source.join("lib/a.rs"),
            "use crate::b::B; pub fn make() -> B { B }\n",
        )
        .unwrap();
        fs::write(source.join("lib/b.rs"), "pub struct B;\n").unwrap();

        let library = Library::load(&source.join("lib.rs")).unwrap();
        let main = syn::parse_file("use sample::a::make; fn main() { let _ = make(); }").unwrap();
        let selected = select_library(&main, &library, "sample");
        assert!(selected
            .modules
            .contains(&vec!["a".into()]));
        assert!(selected
            .modules
            .contains(&vec!["b".into()]));
    }

    #[test]
    fn keeps_only_reachable_items_for_an_explicit_function_import() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory
            .path()
            .join("src");
        fs::create_dir_all(source.join("lib")).unwrap();
        fs::write(
            source.join("lib.rs"),
            "#[path = \"lib/string.rs\"] pub mod string;\n",
        )
        .unwrap();
        fs::write(
            source.join("lib/string.rs"),
            "\
pub fn manacher() -> usize { helper() }
fn helper() -> usize { 1 }
pub trait SequenceExt { fn is_palindrome(&self) -> bool; }
impl SequenceExt for str { fn is_palindrome(&self) -> bool { true } }
pub trait DecimalDigitsExt { fn parse_usize(&self) -> Option<usize>; }
",
        )
        .unwrap();

        let library = Library::load(&source.join("lib.rs")).unwrap();
        let main =
            syn::parse_file("use sample::string::manacher; fn main() { let _ = manacher(); }")
                .unwrap();
        let selected = select_library(&main, &library, "sample");
        let path = vec!["string".into()];
        let declaration = library
            .root
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Mod(module) => Some(module),
                _ => None,
            });
        let module = inline_module(&path, declaration.unwrap(), &library, &selected).unwrap();
        let names = module
            .content
            .unwrap()
            .1
            .iter()
            .filter_map(item_name)
            .collect::<Vec<_>>();

        assert_eq!(names, ["manacher", "helper"]);
    }

    #[test]
    fn resolves_dependencies_from_actual_annealing_module() {
        let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let library = Library::load(&project.join("src/lib.rs")).unwrap();
        let main = syn::parse_file(
            "use atcoder_heuristic::annealing::{anneal, AnnealConfig}; fn main() {}",
        )
        .unwrap();
        let selected = select_library(&main, &library, "atcoder_heuristic");

        assert!(selected
            .modules
            .contains(&vec!["annealing".into()]));
        assert!(selected
            .modules
            .contains(&vec!["random".into()]));
        assert!(selected
            .modules
            .contains(&vec!["timer".into()]));
    }

    #[test]
    fn bundles_root_reexported_input_and_output() {
        let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let library = Library::load(&project.join("src/lib.rs")).unwrap();
        let crate_name = "atcoder_heuristic";
        let mut main = syn::parse_file(
            "use atcoder_heuristic::{Input, Output}; fn main() { let mut input = Input::new(); let _ = input.has_next(); let mut output = Output::new(); output.join_space([0, 1]); output.yes_no(true); output.flush(); }",
        )
        .unwrap();
        let root_reexports = used_root_reexports(&main, &library, crate_name);
        let selected = select_library(&main, &library, crate_name);
        assert!(root_reexports.contains("Input"));
        assert!(root_reexports.contains("Output"));
        assert!(selected
            .modules
            .contains(&vec!["io".into()]));

        remove_root_reexport_imports(&mut main.items, crate_name, &root_reexports);
        let mut items = root_reexport_items(&root_reexports, &library).unwrap();
        let declaration = library
            .root
            .items
            .iter()
            .find_map(|item| match item {
                Item::Mod(module) if module.ident == "io" => Some(module),
                _ => None,
            })
            .unwrap();
        items.push(Item::Mod(
            inline_module(&["io".into()], declaration, &library, &selected).unwrap(),
        ));
        items.extend(main.items);
        let bundle = prettyplease::unparse(&syn::File {
            shebang: None,
            attrs: Vec::new(),
            items,
        });
        let directory = tempfile::tempdir().unwrap();
        let output = directory
            .path()
            .join("bundle.rs");
        fs::write(&output, bundle).unwrap();
        let manifest = toml::from_str("[dependencies]\n").unwrap();
        compile_with_cargo(&output, "2021", &manifest, directory.path()).unwrap();
    }

    #[test]
    fn removes_only_exported_macros_from_grouped_import() {
        let item: syn::ItemUse = syn::parse_quote!(
            use atcoder::{input, io::Writer, geometry::Point};
        );
        let macros = [("input".into(), vec!["io".into()])]
            .into_iter()
            .collect();
        let filtered =
            filter_macro_imports(&item.tree, &mut Vec::new(), "atcoder", &macros).unwrap();
        let mut paths = Vec::new();
        collect_use_tree(&filtered, &mut Vec::new(), &mut paths);
        assert_eq!(
            paths,
            [
                vec!["atcoder", "io", "Writer"],
                vec!["atcoder", "geometry", "Point"]
            ]
        );
    }

    #[test]
    fn compiles_bundle_with_package_dependencies() {
        let directory = tempfile::tempdir().unwrap();
        let dependency = directory
            .path()
            .join("dependency");
        fs::create_dir_all(dependency.join("src")).unwrap();
        fs::write(
            dependency.join("Cargo.toml"),
            "[package]\nname = \"sample-dependency\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(
            dependency.join("src/lib.rs"),
            "pub fn answer() -> usize { 42 }\n",
        )
        .unwrap();

        let bundle = directory
            .path()
            .join("bundle.rs");
        fs::write(
            &bundle,
            "fn main() { assert_eq!(sample_dependency::answer(), 42); }\n",
        )
        .unwrap();
        let manifest = toml::from_str(&format!(
            "[dependencies]\nsample-dependency = {{ path = {:?} }}\n",
            dependency
        ))
        .unwrap();

        compile_with_cargo(&bundle, "2021", &manifest, directory.path()).unwrap();
    }
}
