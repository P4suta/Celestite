use crate::{Document, DocumentError};
use celestite_archive::{heading_offset, link_at};
use celestite_core::prefer_web_link;
use percent_encoding::percent_decode_str;
use std::{
    fs,
    path::{Path, PathBuf},
};
use url::Url;

const HTTPS_PREFIX: &str = "https://";

#[derive(Clone, Copy)]
enum LocalLinkKind {
    Note,
    Attachment,
}

const ATTACHMENT_EXTENSIONS: [&str; 13] = [
    "pdf", "png", "jpg", "jpeg", "gif", "webp", "avif", "heic", "mp3", "m4a", "wav", "mp4", "mov",
];

fn local_link_kind(extension: &str) -> Option<LocalLinkKind> {
    match extension {
        "md" | "markdown" | "txt" => Some(LocalLinkKind::Note),
        other if ATTACHMENT_EXTENSIONS.contains(&other) => Some(LocalLinkKind::Attachment),
        _ => None,
    }
}

/// Returns the lowercase library spelling of a supported media extension, with `jpeg` written as `jpg`.
pub fn asset_extension(extension: &str) -> Option<&'static str> {
    let lower = extension.to_ascii_lowercase();
    let canonical = if lower == "jpeg" { "jpg" } else { &lower };
    ATTACHMENT_EXTENSIONS
        .into_iter()
        .find(|known| *known == canonical)
}

fn scheme_less_url(destination: &str) -> Option<Url> {
    let explicit_scheme = destination
        .split_once("://")
        .is_some_and(|(prefix, _)| !prefix.contains(['/', '?', '#']));
    if destination.starts_with(['.', '/', '\\', '?'])
        || explicit_scheme
        || destination
            .chars()
            .any(|value| value.is_control() || value == '\\')
    {
        return None;
    }
    let url = Url::parse(&format!("{HTTPS_PREFIX}{destination}")).ok()?;
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let domain = url.domain()?;
    let host = idna::domain_to_ascii_strict(domain.strip_suffix('.').unwrap_or(domain)).ok()?;
    if !host.trim_end_matches('.').contains('.') {
        return None;
    }
    Some(url)
}

fn local_entry_exists(path: Option<&Path>, destination: &str) -> Result<bool, DocumentError> {
    let Some(parent) = path.and_then(Path::parent) else {
        return Ok(false);
    };
    let component = destination
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let decoded = percent_decode_str(component)
        .decode_utf8()
        .map_err(|_| DocumentError::Link("The link destination is not valid UTF-8."))?;
    match fs::symlink_metadata(parent.join(decoded.as_ref())) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[derive(Debug)]
pub enum LinkNavigation {
    Here {
        selection: usize,
    },
    Note {
        document: Box<Document>,
        selection: Option<usize>,
    },
    Web {
        url: String,
    },
    Attachment {
        path: PathBuf,
    },
}

pub(crate) fn byte_offset(source: &str, position: usize) -> Option<usize> {
    let mut units = 0;
    for (offset, character) in source.char_indices() {
        if units == position {
            return Some(offset);
        }
        units += character.len_utf16();
        if units > position {
            return None;
        }
    }
    (units == position).then_some(source.len())
}

fn selection(source: &str, fragment: &str) -> Result<usize, DocumentError> {
    let fragment = percent_decode_str(fragment)
        .decode_utf8()
        .map_err(|_| DocumentError::Link("The heading link is not valid UTF-8."))?;
    let position = heading_offset(source, &fragment).ok_or(DocumentError::Link(
        "The linked heading could not be found.",
    ))?;
    Ok(source
        .get(..position)
        .ok_or(DocumentError::Link("Invalid heading position."))?
        .encode_utf16()
        .count())
}

pub(crate) fn resolve(
    source: &str,
    path: Option<&Path>,
    position: usize,
) -> Result<LinkNavigation, DocumentError> {
    let destination = link_at(source, position).ok_or(DocumentError::Link(
        "Place the cursor on a Markdown link to open it.",
    ))?;
    if let Some(fragment) = destination.strip_prefix('#') {
        return Ok(LinkNavigation::Here {
            selection: selection(source, fragment)?,
        });
    }
    if destination.is_empty() {
        return Ok(LinkNavigation::Here { selection: 0 });
    }
    let inferred = scheme_less_url(&destination);
    let local_suffix = inferred
        .as_ref()
        .and_then(Url::domain)
        .and_then(|domain| {
            domain
                .strip_suffix('.')
                .unwrap_or(domain)
                .rsplit('.')
                .next()
        })
        .and_then(local_link_kind)
        .is_some();
    let local_entry =
        inferred.is_some() && !local_suffix && local_entry_exists(path, &destination)?;
    if prefer_web_link(inferred.is_some(), local_suffix, local_entry)
        && let Some(url) = inferred
    {
        return Ok(LinkNavigation::Web { url: url.into() });
    }
    let url = match Url::parse(&destination) {
        Ok(url) => url,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let path = path.ok_or(DocumentError::Link(
                "Save this note before opening a relative link.",
            ))?;
            Url::from_file_path(path)
                .map_err(|()| DocumentError::InvalidPath)?
                .join(&destination)
                .map_err(|_| DocumentError::Link("The link destination is invalid."))?
        }
        Err(_) => return Err(DocumentError::Link("The link destination is invalid.")),
    };
    if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() {
        return Ok(LinkNavigation::Web { url: url.into() });
    }
    if url.scheme() != "file" || url.host_str().is_some_and(|host| host != "localhost") {
        return Err(DocumentError::Link("This link type is not supported."));
    }
    if url.query().is_some() {
        return Err(DocumentError::Link(
            "Local file links cannot contain a query.",
        ));
    }
    let candidate = url
        .to_file_path()
        .map_err(|()| DocumentError::InvalidPath)?;
    let canonical = match fs::canonicalize(candidate) {
        Ok(canonical) => canonical,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && celestite_archive::note_filename_identity(&destination).is_some() =>
        {
            return Err(DocumentError::Link("This note is not in the library."));
        }
        Err(error) => return Err(error.into()),
    };
    if !canonical.is_file() {
        return Err(DocumentError::InvalidPath);
    }
    if path == Some(canonical.as_path()) {
        return Ok(LinkNavigation::Here {
            selection: selection(source, url.fragment().unwrap_or_default())?,
        });
    }
    let extension = canonical
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match local_link_kind(&extension) {
        Some(LocalLinkKind::Note) => {
            let document = Document::open(&canonical)?;
            let selection = url
                .fragment()
                .map(|fragment| selection(&document.snapshot().text, fragment))
                .transpose()?;
            Ok(LinkNavigation::Note {
                document: Box::new(document),
                selection,
            })
        }
        Some(LocalLinkKind::Attachment) => Ok(LinkNavigation::Attachment { path: canonical }),
        None => Err(DocumentError::Link(
            "Only notes, PDF documents, images, audio, and video can be opened.",
        )),
    }
}
