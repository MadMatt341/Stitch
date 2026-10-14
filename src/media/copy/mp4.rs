use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

struct Atom {
    kind: [u8; 4],
    body: u64,
    end: u64,
}

fn atoms(file: &mut File, start: u64, end: u64, count: &mut usize) -> std::io::Result<Vec<Atom>> {
    let mut result = Vec::new();
    let mut position = start;
    while position < end {
        *count += 1;
        if *count > 100_000 || end - position < 8 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        file.seek(SeekFrom::Start(position))?;
        let mut header = [0; 8];
        file.read_exact(&mut header)?;
        let short = u32::from_be_bytes(header[..4].try_into().unwrap()) as u64;
        let mut header_size = 8;
        let size = if short == 1 {
            let mut extended = [0; 8];
            file.read_exact(&mut extended)?;
            header_size = 16;
            u64::from_be_bytes(extended)
        } else if short == 0 {
            end - position
        } else {
            short
        };
        if size < header_size || size > end - position {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        result.push(Atom {
            kind: header[4..].try_into().unwrap(),
            body: position + header_size,
            end: position + size,
        });
        position += size;
    }
    Ok(result)
}

fn bytes<const N: usize>(file: &mut File, atom: &Atom, offset: u64) -> std::io::Result<[u8; N]> {
    if atom.end - atom.body < offset + N as u64 {
        return Err(std::io::ErrorKind::UnexpectedEof.into());
    }
    file.seek(SeekFrom::Start(atom.body + offset))?;
    let mut value = [0; N];
    file.read_exact(&mut value)?;
    Ok(value)
}

pub(super) fn audio_safe(path: &Path, track_id: Option<u32>) -> bool {
    fn inspect(path: &Path, track_id: u32) -> std::io::Result<bool> {
        let mut file = File::open(path)?;
        let length = file.metadata()?.len();
        let mut count = 0;
        let root = atoms(&mut file, 0, length, &mut count)?;
        let Some(moov) = root.iter().find(|atom| &atom.kind == b"moov") else {
            return Ok(false);
        };
        for track in atoms(&mut file, moov.body, moov.end, &mut count)?
            .iter()
            .filter(|atom| &atom.kind == b"trak")
        {
            let children = atoms(&mut file, track.body, track.end, &mut count)?;
            let Some(header) = children.iter().find(|atom| &atom.kind == b"tkhd") else {
                return Ok(false);
            };
            let version = bytes::<1>(&mut file, header, 0)?[0];
            let offset = match version {
                0 => 12,
                1 => 20,
                _ => return Ok(false),
            };
            if u32::from_be_bytes(bytes(&mut file, header, offset)?) != track_id {
                continue;
            }
            if let Some(edts) = children.iter().find(|atom| &atom.kind == b"edts") {
                for edit in atoms(&mut file, edts.body, edts.end, &mut count)?
                    .iter()
                    .filter(|atom| &atom.kind == b"elst")
                {
                    let version = bytes::<1>(&mut file, edit, 0)?[0];
                    let size = match version {
                        0 => 12,
                        1 => 20,
                        _ => return Ok(false),
                    };
                    if u32::from_be_bytes(bytes(&mut file, edit, 4)?) != 1
                        || edit.end - edit.body < 8 + size
                    {
                        return Ok(false);
                    }
                }
            }
            let Some(mdia) = children.iter().find(|atom| &atom.kind == b"mdia") else {
                return Ok(false);
            };
            let media = atoms(&mut file, mdia.body, mdia.end, &mut count)?;
            let Some(handler) = media.iter().find(|atom| &atom.kind == b"hdlr") else {
                return Ok(false);
            };
            if &bytes::<4>(&mut file, handler, 8)? != b"soun" {
                return Ok(false);
            }
            let Some(minf) = media.iter().find(|atom| &atom.kind == b"minf") else {
                return Ok(false);
            };
            let info = atoms(&mut file, minf.body, minf.end, &mut count)?;
            let Some(stbl) = info.iter().find(|atom| &atom.kind == b"stbl") else {
                return Ok(false);
            };
            let table = atoms(&mut file, stbl.body, stbl.end, &mut count)?;
            let Some(description) = table.iter().find(|atom| &atom.kind == b"stsd") else {
                return Ok(false);
            };
            return Ok(u32::from_be_bytes(bytes(&mut file, description, 4)?) == 1);
        }
        Ok(false)
    }
    track_id.is_some_and(|track| inspect(path, track).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        [
            ((body.len() + 8) as u32).to_be_bytes().as_slice(),
            kind.as_slice(),
            body,
        ]
        .concat()
    }
    fn fixture(edits: u32) -> Vec<u8> {
        let mut header = [0; 16];
        header[12..].copy_from_slice(&2_u32.to_be_bytes());
        let mut edit = vec![0; 8 + edits as usize * 12];
        edit[4..8].copy_from_slice(&edits.to_be_bytes());
        let handler = [vec![0; 8], b"soun".to_vec()].concat();
        let description = [0_u32.to_be_bytes(), 1_u32.to_be_bytes()].concat();
        let media = [
            atom(b"hdlr", &handler),
            atom(b"minf", &atom(b"stbl", &atom(b"stsd", &description))),
        ]
        .concat();
        let track = [
            atom(b"tkhd", &header),
            atom(b"edts", &atom(b"elst", &edit)),
            atom(b"mdia", &media),
        ]
        .concat();
        atom(b"moov", &atom(b"trak", &track))
    }
    #[test]
    fn selected_audio_needs_one_description_and_no_internal_edit_list() {
        let folder = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("build/test-output");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!("audio-edit-list-{}.mp4", std::process::id()));
        std::fs::write(&path, fixture(1)).unwrap();
        assert!(audio_safe(&path, Some(2)));
        assert!(!audio_safe(&path, Some(3)));
        std::fs::write(&path, fixture(2)).unwrap();
        assert!(!audio_safe(&path, Some(2)));
        std::fs::write(&path, &fixture(1)[..12]).unwrap();
        assert!(!audio_safe(&path, Some(2)));
        std::fs::remove_file(&path).unwrap();
    }
}
