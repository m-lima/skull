#![deny(warnings, clippy::pedantic, clippy::all, rust_2018_idioms)]

mod args;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), String> {
    let args = args::parse();

    let store = store::Store::new(args.output, 1)
        .await
        .map_err(|e| e.to_string())?;
    store.migrate().await.map_err(|e| e.to_string())?;

    let skulls = make_path(&args.input, "skull")?;
    let occurrences = make_path(&args.input, "occurrence")?;

    let skulls = ingest_skulls(skulls, &store).await?;

    ingest_occurrences(occurrences, &store, &skulls).await?;

    Ok(())
}

fn make_path(root: &std::path::Path, file: &str) -> Result<std::path::PathBuf, String> {
    let path = root.join(file);
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("File `{file}` does not exist"))
    }
}

async fn ingest_skulls(
    skulls: std::path::PathBuf,
    store: &store::Store,
) -> Result<std::collections::HashMap<types::Id, types::SkullId>, String> {
    let lines = read(skulls)?;
    let mut output = std::collections::HashMap::with_capacity(lines.len());

    for (i, line) in lines.into_iter().enumerate() {
        let i = i + 1;
        let split = line.split('\t').collect::<Vec<_>>();
        let split = match <[&str; 6]>::try_from(split) {
            Ok(split) => split,
            Err(split) => {
                return Err(format!(
                    "Skulls: Line {i}: Expected 6 columns but got {}",
                    split.len()
                ));
            }
        };

        let orig_id = split[0]
            .parse()
            .map_err(|e| format!("Skulls: Line {i} column 1: value is not `i64`: {e}"))?;

        let name = String::from(split[1]);
        let color = u32::from_str_radix(
            split[2]
                .strip_prefix('#')
                .filter(|h| h.len() == 6)
                .ok_or_else(|| {
                    format!(
                        "Skulls: Line {i} column 3: Expected a `#xxxxxx` color format but got {}",
                        split[2]
                    )
                })?,
            16,
        )
        .map_err(|e| format!("Skulls: Line {i} column 3: {e}"))?;
        let icon = String::from(split[3]);
        let price = split[4]
            .parse()
            .map_err(|e| format!("Skulls: Line {i} column 5: value is not `f32`: {e}"))?;
        let limit = split[5]
            .parse()
            .map(Some)
            .map_err(|e| format!("Skulls: Line {i} column 6: value is not `f32`: {e}"))?;

        let new_id = store
            .skulls()
            .create(name, color, icon, price, limit)
            .await
            .map_err(|e| format!("Skulls: Line {i}: Failed to write to store: {e}"))?
            .id;

        output.insert(orig_id, new_id);
    }

    Ok(output)
}

async fn ingest_occurrences(
    occurrences: std::path::PathBuf,
    store: &store::Store,
    skulls: &std::collections::HashMap<types::Id, types::SkullId>,
) -> Result<(), String> {
    let lines = read(occurrences)?;
    let mut occurrences = Vec::<(types::SkullId, f32, types::Millis)>::with_capacity(lines.len());

    for (i, line) in lines.into_iter().enumerate() {
        let i = i + 1;
        let split = line.split('\t').collect::<Vec<_>>();
        let split = match <[&str; 4]>::try_from(split) {
            Ok(split) => split,
            Err(split) => {
                return Err(format!(
                    "Occurrences: Line {i}: Expected 4 columns but got {}",
                    split.len()
                ));
            }
        };

        let orig_skull = split[1]
            .parse()
            .map_err(|e| format!("Occurrences: Line {i} column 2: value is not `i64`: {e}"))?;

        let skull = *skulls
            .get(&orig_skull)
            .ok_or_else(|| format!("Occurrences: Line {i}: could not find ID for {orig_skull}"))?;

        let amount = split[2]
            .parse::<f32>()
            .map_err(|e| format!("Occurrences: Line {i} column 3: value is not `f32`: {e}"))?;

        let millis = split[3]
            .parse::<i64>()
            .map(types::Millis::from)
            .map_err(|e| {
                format!("Occurrences: Line {i} column 4: value is not a millis timestamp: {e}")
            })?;

        occurrences.push((skull, amount, millis));
    }

    occurrences.sort_unstable_by(|a, b| match a.2.cmp(&b.2) {
        std::cmp::Ordering::Equal => match a.0.cmp(&b.0) {
            std::cmp::Ordering::Equal => match a.1.partial_cmp(&b.1) {
                Some(o) => o,
                None => panic!(),
            },
            o => o,
        },
        o => o,
    });

    occurrences
        .iter()
        .scan(
            (i64::MIN, f32::NEG_INFINITY, types::Millis::from(i64::MIN)),
            |state, curr| {
                // allow(clippy::cast_sign_loss): this is just for printing
                #[allow(clippy::cast_sign_loss)]
                if curr.2 == state.2
                    && i64::from(curr.0) == state.0
                    && curr.1.to_ne_bytes() == state.1.to_ne_bytes()
                {
                    println!(
                        "Found repeated millis for {} {} {} ({})",
                        curr.0,
                        curr.1,
                        curr.2,
                        chrono::DateTime::from_timestamp(
                            i64::from(curr.2) / 1000,
                            ((i64::from(curr.2) % 1000) as u32) * 1_000_000
                        )
                        .unwrap()
                    );
                }
                *state = (curr.0.into(), curr.1, curr.2);
                Some(true)
            },
        )
        .all(|i| i);

    let mut buffer = Vec::with_capacity(20);

    for occurrence in occurrences {
        if buffer.len() == 20 {
            store
                .occurrences()
                .create(buffer.iter().copied())
                .await
                .map_err(|e| format!("Failed to write to store: {e}"))?;
            buffer.clear();
        }
        buffer.push(occurrence);
    }

    if !buffer.is_empty() {
        store
            .occurrences()
            .create(buffer)
            .await
            .map_err(|e| format!("Failed to write to store: {e}"))?;
    }

    Ok(())
}

fn read(path: std::path::PathBuf) -> Result<Vec<String>, String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|e| e.to_string())?;

    let file = std::io::BufReader::new(file);
    std::io::BufRead::lines(file)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
