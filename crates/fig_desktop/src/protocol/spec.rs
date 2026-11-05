use std::borrow::Cow;
use std::io::ErrorKind;
use std::path::{
    Component,
    Path,
};
use std::sync::Arc;

use fig_os_shim::Context;
use percent_encoding::percent_decode_str;
use wry::http::header::CONTENT_TYPE;
use wry::http::{
    Request,
    Response,
    StatusCode,
};

use super::util::{
    res_400,
    res_404,
    res_500,
};
use crate::webview::WindowId;

fn serve_spec(root: &Path, request: &Request<Vec<u8>>) -> anyhow::Result<Response<Cow<'static, [u8]>>> {
    if request.uri().host() != Some("localhost") {
        return res_400();
    }

    // Strip only the URI's leading slash, then decode once. A second slash or an
    // encoded leading slash must not turn a request into an absolute file path.
    let uri_path = request.uri().path();
    let relative = uri_path.strip_prefix('/').unwrap_or(uri_path);
    let decoded = match percent_decode_str(relative).decode_utf8() {
        Ok(decoded) => decoded,
        Err(_) => return res_400(),
    };
    let relative = Path::new(decoded.as_ref());
    if decoded.contains('\0')
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return res_400();
    }

    let root = match root.canonicalize() {
        Ok(root) => root,
        Err(err) if err.kind() == ErrorKind::NotFound => return res_404(),
        Err(err) => return res_500(err),
    };
    let mut requested_path = root.join(relative);
    let mut symlink_resolutions = 0;
    let path = 'resolve: loop {
        match requested_path.canonicalize() {
            Ok(path) => break path,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                // Inspect missing paths too: their nearest existing ancestor
                // (or a dangling symlink's target) must remain in the bundle.
                for ancestor in requested_path.ancestors() {
                    match ancestor.canonicalize() {
                        Ok(ancestor) if !ancestor.starts_with(&root) => return res_400(),
                        Ok(_) => return res_404(),
                        Err(err) if err.kind() == ErrorKind::NotFound => {},
                        Err(err) => return res_500(err),
                    }
                    match ancestor.symlink_metadata() {
                        Ok(metadata) if metadata.file_type().is_symlink() => {
                            symlink_resolutions += 1;
                            if symlink_resolutions > 40 {
                                return res_500(std::io::Error::other("Too many symbolic links"));
                            }
                            let target = match std::fs::read_link(ancestor) {
                                Ok(target) => target,
                                Err(err) => return res_500(err),
                            };
                            let target = if target.is_absolute() {
                                target
                            } else {
                                ancestor.parent().unwrap_or(&root).join(target)
                            };
                            let suffix = requested_path.strip_prefix(ancestor)?;
                            requested_path = if suffix.as_os_str().is_empty() {
                                target
                            } else {
                                target.join(suffix)
                            };
                            continue 'resolve;
                        },
                        Ok(_) => {},
                        Err(err) if err.kind() == ErrorKind::NotFound => {},
                        Err(err) => return res_500(err),
                    }
                }
                return res_404();
            },
            Err(err) => return res_500(err),
        }
    };
    if !path.starts_with(&root) {
        return res_400();
    }

    let content = match std::fs::read(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == ErrorKind::NotFound => return res_404(),
        Err(err) => return res_500(err),
    };
    let mime = match path.extension().and_then(|extension| extension.to_str()) {
        Some("js") => mime::APPLICATION_JAVASCRIPT_UTF_8.as_ref(),
        Some("json") => mime::APPLICATION_JSON.as_ref(),
        Some("svg") => mime::IMAGE_SVG.as_ref(),
        Some("png") => mime::IMAGE_PNG.as_ref(),
        Some("jpg" | "jpeg") => mime::IMAGE_JPEG.as_ref(),
        _ => mime::APPLICATION_OCTET_STREAM.as_ref(),
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, mime)
        .body(content.into())?)
}

/// Serve the bundled catalogue, including nested and versioned specs.
pub async fn handle(
    ctx: Arc<Context>,
    request: Request<Vec<u8>>,
    _: WindowId,
) -> anyhow::Result<Response<Cow<'static, [u8]>>> {
    let root = fig_util::directories::resources_path_ctx(&ctx)?.join("specs");
    serve_spec(&root, &request)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn request(path: &str) -> Request<Vec<u8>> {
        Request::builder().uri(path).body(Vec::new()).unwrap()
    }

    #[test]
    fn serves_catalogue_index() {
        let root = tempfile::tempdir().unwrap();
        let index = serde_json::json!({
            "completions": ["git", "npm", "aws"],
            "diffVersionedCompletions": ["aws"]
        });
        let bytes = serde_json::to_vec(&index).unwrap();
        fs::write(root.path().join("index.json"), &bytes).unwrap();

        let response = serve_spec(root.path(), &request("spec://localhost/index.json")).unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_TYPE], "application/json");
        assert_eq!(response.body().as_ref(), bytes.as_slice());
        assert_eq!(serde_json::from_slice::<serde_json::Value>(response.body()).unwrap(), index);
    }

    #[test]
    fn serves_javascript_and_nested_versioned_specs() {
        let root = tempfile::tempdir().unwrap();
        let javascript = b"export default { name: 'git' };";
        for name in ["git.js", "aws/index.js", "aws/2.0.0.js", "nested/spec.js", "space name.js", "%2e%2e.js"] {
            let path = root.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, javascript).unwrap();
        }

        for name in ["git.js", "aws/index.js", "aws/2.0.0.js", "nested/spec.js", "space%20name.js", "%252e%252e.js"] {
            let response = serve_spec(root.path(), &request(&format!("spec://localhost/{name}"))).unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{name}");
            assert_eq!(response.headers()[CONTENT_TYPE], "application/javascript; charset=utf-8");
            assert_eq!(response.body().as_ref(), javascript);
        }
    }

    #[test]
    fn serves_bundled_icon_bytes_with_image_mime() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("icons")).unwrap();
        for (name, bytes, mime) in [
            ("file.png", b"\x89PNG\r\n\x1a\n".as_slice(), "image/png"),
            ("folder.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".as_slice(), "image/svg+xml"),
            ("image.jpg", b"\xff\xd8\xff".as_slice(), "image/jpeg"),
        ] {
            fs::write(root.path().join("icons").join(name), bytes).unwrap();
            let response = serve_spec(root.path(), &request(&format!("spec://localhost/icons/{name}"))).unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{name}");
            assert_eq!(response.headers()[CONTENT_TYPE], mime, "{name}");
            assert_eq!(response.body().as_ref(), bytes, "{name}");
        }
    }

    #[test]
    fn missing_catalogue_and_files_are_not_found_without_spa_fallback() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("index.html"), "not a spec").unwrap();
        for name in ["index.json", "missing.js", "missing", "nested/missing.js"] {
            let response = serve_spec(root.path(), &request(&format!("spec://localhost/{name}"))).unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{name}");
            assert!(!response.body().is_empty());
        }
        let response = serve_spec(&root.path().join("absent"), &request("spec://localhost/index.json")).unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn rejects_other_hosts_and_unsafe_paths() {
        let root = tempfile::tempdir().unwrap();
        for uri in [
            "spec://example.com/index.json",
            "spec://localhost.evil/index.json",
            "spec://localhost//etc/passwd",
            "spec://localhost/%2Fetc/passwd",
            "spec://localhost/../outside.js",
            "spec://localhost/%2e%2e/outside.js",
            "spec://localhost/nested/%2E%2E/outside.js",
            "spec://localhost/nested%2f..%2foutside.js",
            "spec://localhost/invalid%00.js",
            "spec://localhost/%ff.js",
            "/index.json",
        ] {
            let response = serve_spec(root.path(), &request(uri)).unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escapes_but_serves_contained_symlinks() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("outside.js"), "private").unwrap();
        fs::write(root.path().join("inside.js"), "export default {};").unwrap();
        symlink(outside.path(), root.path().join("escape")).unwrap();
        symlink(root.path().join("inside.js"), root.path().join("alias.js")).unwrap();
        symlink(outside.path().join("missing.js"), root.path().join("dangling.js")).unwrap();
        symlink("dangling.js", root.path().join("chained.js")).unwrap();
        symlink("missing.js", root.path().join("contained-missing.js")).unwrap();
        symlink(outside.path().join("missing"), root.path().join("dangling-directory")).unwrap();

        for name in [
            "escape/outside.js",
            "escape/missing.js",
            "dangling.js",
            "chained.js",
            "dangling-directory/child.js",
        ] {
            let response = serve_spec(root.path(), &request(&format!("spec://localhost/{name}"))).unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{name}");
        }
        let response = serve_spec(root.path(), &request("spec://localhost/alias.js")).unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = serve_spec(root.path(), &request("spec://localhost/contained-missing.js")).unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn read_errors_are_server_errors() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("directory.js")).unwrap();
        let response = serve_spec(root.path(), &request("spec://localhost/directory.js")).unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
