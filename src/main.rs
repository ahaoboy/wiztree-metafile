use serde_metafile::{Import, Input, InputDetail, Metafile, Output};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

fn get_inputs(root: &PathBuf, dir: &str) -> HashMap<String, Input> {
    let mut hm = HashMap::new();
    for entry in std::fs::read_dir(dir).unwrap().flat_map(Result::ok) {
        let path = entry.path();
        let full_path = path.canonicalize().unwrap();
        let id = full_path
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
            .replace("\\", "/");
        let meta = entry.metadata().unwrap();
        if meta.is_dir() {
            let sub_inputs = get_inputs(root, path.to_str().unwrap());
            let mut imports = vec![];
            let mut bytes = 0;
            for (id, v) in &sub_inputs {
                imports.push(Import {
                    path: id.clone(),
                    kind: None,
                    external: false,
                    original: None,
                    with: None,
                });
                bytes += v.bytes;
            }
            hm.insert(
                id,
                Input {
                    bytes: bytes + meta.len(),
                    imports,
                    format: None,
                    with: None,
                },
            );
            hm.extend(sub_inputs);
        } else if meta.is_file() {
            hm.insert(
                id,
                Input {
                    bytes: meta.len(),
                    imports: vec![],
                    format: None,
                    with: None,
                },
            );
        }
    }

    hm
}

fn metasize(root: &str) -> Metafile {
    let root_path = Path::new(root).canonicalize().unwrap();
    let mut inputs = get_inputs(&root_path, root);
    let root_name = root_path.file_name().unwrap().to_str().unwrap().to_string();
    let mut outputs = HashMap::new();
    let mut output_inputs = HashMap::new();
    let mut bytes = 0;
    let mut imports = vec![];
    for i in std::fs::read_dir(root).unwrap().flat_map(Result::ok) {
        let full_path = i.path().canonicalize().unwrap();
        let id = full_path
            .strip_prefix(&root_path)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
            .replace("\\", "/");

        if let Some(input) = inputs.get(&id) {
            bytes += input.bytes;
        }
        imports.push(Import {
            path: id,
            kind: None,
            external: false,
            original: None,
            with: None,
        });
    }

    for (id, v) in &inputs {
        output_inputs.insert(
            id.clone(),
            InputDetail {
                bytes_in_output: v.bytes,
            },
        );
    }
    inputs.insert(
        root_name.clone(),
        Input {
            bytes,
            imports,
            format: None,
            with: None,
        },
    );
    let output = Output {
        bytes,
        inputs: output_inputs,
        imports: vec![],
        exports: vec![],
        entry_point: Some(root_name.clone()),
        css_bundle: None,
    };
    let root_name = root_path.file_name().unwrap().to_str().unwrap().to_string();
    outputs.insert(root_name, output);
    Metafile { inputs, outputs }
}

fn main() {
    let Some(folder) = std::env::args().nth(1) else {
        println!("wiztree-metafile <FOLDER>");
        return;
    };

    let meta = metasize(&folder);
    let s = serde_json::to_string(&meta).unwrap();
    println!("{s}");
}
