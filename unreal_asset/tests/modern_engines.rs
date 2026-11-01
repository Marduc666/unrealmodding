//! Round trips cooked UE 5.3+ packages. The packages belong to their games so they aren't checked
//! in, point `UE_TEST_ASSETS` at a folder of `suite` subfolders to run this
//!
//! ```text
//! UE_TEST_ASSETS=/path/to/UAssetAPI.Tests/TestAssets cargo test --test modern_engines -- --nocapture
//! ```

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use unreal_asset::{
    cast,
    engine_version::EngineVersion,
    exports::ExportBaseTrait,
    schema::{pull_schemas, LooseContent},
    types::PackageIndex,
    unversioned::Usmap,
    Asset, Export,
};

const SUITES: &[(&str, EngineVersion, &str)] = &[
    ("TestUE5_3/Engine", EngineVersion::VER_UE5_3, "Engine.usmap"),
    ("TestUE5_5/BlankGame", EngineVersion::VER_UE5_5, "BlankUE5_5.usmap"),
    ("TestUE5_6/Grounded2", EngineVersion::VER_UE5_6, "Mappings.usmap"),
    ("TestUE5_7/NanosWorld", EngineVersion::VER_UE5_7, "NanosWorld.usmap"),
    ("TestUE5_7/FarFarWest", EngineVersion::VER_UE5_7, "FarFarWest.usmap"),
    ("TestUE5_7/Grounded2", EngineVersion::VER_UE5_7, "Grounded2_0.6.0.usmap"),
];

fn packages(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            packages(&path, found);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("umap" | "uasset")
        ) {
            found.push(path);
        }
    }
}

#[derive(Default)]
struct Report {
    parsed: usize,
    failed: usize,
    identical: usize,
    edited: usize,
    panics: usize,
    actors: usize,
    raw_actors: usize,
    raw_actor_classes: BTreeMap<String, usize>,
    maps: usize,
    maps_identical: usize,
    exports: usize,
    raw_exports: usize,
    errors: BTreeMap<String, usize>,
    raw_classes: BTreeMap<String, usize>,
}

struct Context {
    usmap: Usmap,
    content: Option<LooseContent>,
    visited: HashSet<String>,
}

fn round_trip(path: &Path, version: EngineVersion, context: &mut Context, report: &mut Report) {
    let data = fs::read(path).unwrap();
    let bulk = fs::read(path.with_extension("uexp")).ok();
    let read = |usmap: &Usmap| {
        Asset::new(
            Cursor::new(data.as_slice()),
            bulk.as_deref().map(Cursor::new),
            version,
            Some(usmap.clone()),
        )
    };
    let mut asset = read(&context.usmap);
    if let (Ok(first), Some(content)) = (&asset, &context.content) {
        let added = pull_schemas(
            first,
            version,
            &mut context.usmap,
            &mut context.visited,
            |package| content.load(package),
        );
        if added > 0 {
            asset = read(&context.usmap);
        }
    }
    let asset = match asset {
        Ok(asset) => asset,
        Err(e) => {
            report.failed += 1;
            let message = e.to_string();
            *report
                .errors
                .entry(message.chars().take(100).collect())
                .or_default() += 1;
            return;
        }
    };
    report.parsed += 1;

    let class_of = |export: &Export<PackageIndex>| {
        asset
            .get_import(export.get_base_export().class_index)
            .map(|import| import.object_name.get_owned_content())
            .unwrap_or_else(|| "(export class)".into())
    };
    if let Some(level) = asset
        .asset_data
        .exports
        .iter()
        .find_map(|e| cast!(Export, LevelExport, e))
    {
        for actor in &level.actors {
            let Some(export) = asset.get_export(*actor) else {
                continue;
            };
            report.actors += 1;
            if cast!(Export, RawExport, export).is_some() {
                report.raw_actors += 1;
                *report.raw_actor_classes.entry(class_of(export)).or_default() += 1;
            }
        }
    }

    for export in &asset.asset_data.exports {
        report.exports += 1;
        if cast!(Export, RawExport, export).is_some() {
            report.raw_exports += 1;
            *report.raw_classes.entry(class_of(export)).or_default() += 1;
        }
    }

    let mut out = Cursor::new(Vec::new());
    let mut out_bulk = bulk.is_some().then(|| Cursor::new(Vec::new()));
    if let Err(e) = asset.write_data(&mut out, out_bulk.as_mut()) {
        let message: String = e.to_string().chars().take(240).collect();
        *report.errors.entry(format!("write failed: {message}")).or_default() += 1;
        return;
    }
    let header_same = out.get_ref() == &data;
    let out_bulk = out_bulk.map(|e| e.into_inner());
    let bulk_same = out_bulk == bulk;
    if !bulk_same {
        if let (Ok(dir), Some(a)) = (std::env::var("UE_TEST_DUMP"), &out_bulk) {
            fs::create_dir_all(&dir).unwrap();
            fs::write(Path::new(&dir).join(path.with_extension("uexp").file_name().unwrap()), a).unwrap();
        }
        if let (Some(a), Some(b)) = (&out_bulk, &bulk) {
            let at = a.iter().zip(b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
            println!("  {}: bulk {} vs {}, first diff {at}", path.display(), a.len(), b.len());
        }
    }
    if !header_same {
        if let Ok(dir) = std::env::var("UE_TEST_DUMP") {
            let name = path.file_name().unwrap();
            fs::create_dir_all(&dir).unwrap();
            fs::write(Path::new(&dir).join(name), out.get_ref()).unwrap();
        }
        let a = out.get_ref();
        let at = a.iter().zip(&data).position(|(x, y)| x != y).unwrap_or(a.len().min(data.len()));
        println!("  {}: header {} vs {}, first diff {at}", path.display(), a.len(), data.len());
    }
    let is_map = path.extension().is_some_and(|e| e == "umap");
    report.maps += is_map as usize;
    if header_same && bulk_same {
        report.identical += 1;
        report.maps_identical += is_map as usize;
    } else {
        *report
            .errors
            .entry(format!("differs (header same: {header_same}, exports same: {bulk_same})"))
            .or_default() += 1;
    }

    // saving after an edit moves every table, the result still has to read back the same
    let mut asset = asset;
    asset.add_fname("StoveEditedName");
    let mut out = Cursor::new(Vec::new());
    let mut out_bulk = bulk.is_some().then(|| Cursor::new(Vec::new()));
    if asset.write_data(&mut out, out_bulk.as_mut()).is_err() {
        return;
    }
    let mappings = context.usmap.clone();
    let out_bulk = out_bulk.map(|e| e.into_inner());
    let out = out.into_inner();
    let reread = Asset::new(
        Cursor::new(out.as_slice()),
        out_bulk.as_deref().map(Cursor::new),
        version,
        Some(mappings),
    );
    match reread {
        Ok(reread)
            if reread.imports == asset.imports
                && reread.asset_data.exports.len() == asset.asset_data.exports.len()
                && reread.search_name_reference("StoveEditedName").is_some() =>
        {
            report.edited += 1
        }
        Ok(_) => {
            *report.errors.entry("reread differs".into()).or_default() += 1;
        }
        Err(e) => {
            let message: String = e.to_string().chars().take(100).collect();
            *report.errors.entry(format!("reread failed: {message}")).or_default() += 1;
        }
    }
}

fn run_suite(
    suite: &str,
    dir: &Path,
    version: EngineVersion,
    usmap: &Path,
    only_maps: bool,
    content: Option<LooseContent>,
) {
    let usmap = Usmap::new(Cursor::new(fs::read(usmap).unwrap())).expect("mappings should parse");
    let mut context = Context {
        usmap,
        content,
        visited: HashSet::new(),
    };
    let mut found = Vec::new();
    packages(dir, &mut found);
    if only_maps {
        found.retain(|path| path.extension().is_some_and(|e| e == "umap"));
    }
    if let Ok(only) = std::env::var("UE_TEST_ONLY") {
        found.retain(|path| path.to_string_lossy().contains(&only));
    }
    let mut report = Report::default();
    for path in &found {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            round_trip(path, version, &mut context, &mut report)
        }));
        if outcome.is_err() {
            report.panics += 1;
            println!("  panic in {}", path.display());
        }
    }
    println!(
        "{suite}: {} packages, {} parsed, {} failed, {} panicked, {} byte identical, {}/{} maps identical, {} read back after edit, {}/{} exports raw, {}/{} actors raw",
        found.len(),
        report.parsed,
        report.failed,
        report.panics,
        report.identical,
        report.maps_identical,
        report.maps,
        report.edited,
        report.raw_exports,
        report.exports,
        report.raw_actors,
        report.actors,
    );
    let mut errors: Vec<_> = report.errors.into_iter().collect();
    errors.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (message, count) in errors.iter().take(12) {
        println!("    {count:>5}x {message}");
    }
    let mut classes: Vec<_> = report.raw_classes.into_iter().collect();
    classes.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (class, count) in classes.iter().take(12) {
        println!("    raw {count:>5}x {class}");
    }
    if std::env::var_os("UE_TEST_ONLY").is_some() {
        for class in report.raw_actor_classes.keys() {
            let mut chain = String::new();
            let mut name = class.as_str();
            loop {
                let Some(schema) = context.usmap.schemas.get_by_key(name) else {
                    chain += &format!(" -> MISSING {name}");
                    break;
                };
                chain += &format!(" -> {name}[{}/{}]", schema.properties.len(), schema.prop_count);
                name = schema.super_type.as_str();
            }
            println!("CHAIN{chain}");
        }
    }
    let mut classes: Vec<_> = report.raw_actor_classes.into_iter().collect();
    classes.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (class, count) in classes.iter().take(12) {
        println!("    raw actor {count:>5}x {class}");
    }
}

#[test]
fn modern_engines() {
    if let Ok(root) = std::env::var("UE_TEST_ASSETS") {
        for (suite, version, usmap) in SUITES {
            let dir = Path::new(&root).join(suite);
            if dir.exists() {
                run_suite(suite, &dir, *version, &dir.join(usmap), false, None);
            }
        }
    }
    // a game extracted with retoc, UE_TEST_GAME=<folder>;<usmap>;<game>;<maps|all|blueprints>
    if let Ok(game) = std::env::var("UE_TEST_GAME") {
        let mut game = game.split(';');
        let (dir, usmap) = (Path::new(game.next().unwrap()), game.next().unwrap());
        let content = LooseContent::new(dir, game.next().unwrap());
        run_suite(
            "game",
            dir,
            EngineVersion::VER_UE5_8,
            Path::new(usmap),
            game.next() != Some("all"),
            Some(content),
        );
    }
}
