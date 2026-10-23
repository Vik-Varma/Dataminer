//! End-to-end fetches against local git repos and a local BDS index.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

/// Fixture repos and index shaped like the real ones:
/// protocol-docs `v1.26.40` (2190), `v1.26.45` (2191), `v1.26.50` and
/// `v1.26.51` (both 2193); samples `v1.26.40.05` and `v1.26.50.4`; a BDS index
/// with a good 1.26.51 entry, a 1.26.50 entry with the wrong hash, and a
/// 1.26.45 entry pointing off Mojang's host.
struct Fixture {
    root: PathBuf,
    remotes: fn(&Path) -> Remotes,
}

fn fixture(name: &str) -> Fixture {
    let root = test_dir(name);

    let docs = root.join("docs");
    init(&docs);
    for (tag, protocol) in [
        ("v1.26.40", 2190),
        ("v1.26.45", 2191),
        ("v1.26.50", 2193),
        ("v1.26.51", 2193),
    ] {
        let packet = format!(r#"{{"title":"AnimatePacket","x-protocol-version":{protocol}}}"#);
        std::fs::create_dir_all(docs.join("json")).unwrap();
        std::fs::write(docs.join("json/AnimatePacket.json"), packet).unwrap();
        std::fs::write(docs.join("README.md"), tag).unwrap();
        commit_and_tag(&docs, tag);
    }
    // 1.26.52 exists only on Mojang's bot branch, untagged.
    git_in(&docs, &["checkout", "--quiet", "-b", "automated/1.26.52"]);
    std::fs::write(docs.join("README.md"), "1.26.52").unwrap();
    git_in(
        &docs,
        &["commit", "--quiet", "--all", "--message", "1.26.52"],
    );
    git_in(&docs, &["checkout", "--quiet", "main"]);
    // A tag whose packets state no protocol, to make a docs fetch fail late.
    git_in(&docs, &["checkout", "--quiet", "-b", "broken"]);
    std::fs::write(docs.join("json/AnimatePacket.json"), "{}").unwrap();
    commit_and_tag(&docs, "no-protocol");
    git_in(&docs, &["checkout", "--quiet", "main"]);

    let samples = root.join("samples");
    init(&samples);
    for (tag, version) in [("v1.26.40.05", "1.26.40.5"), ("v1.26.50.4", "1.26.50.4")] {
        std::fs::create_dir_all(samples.join("metadata/vanilladata_modules")).unwrap();
        std::fs::create_dir_all(samples.join("behavior_pack")).unwrap();
        std::fs::write(
            samples.join("metadata/vanilladata_modules/mojang-blocks.json"),
            version,
        )
        .unwrap();
        std::fs::write(samples.join("behavior_pack/big.json"), "not wanted").unwrap();
        std::fs::write(
            samples.join("version.json"),
            format!(r#"{{"latest":{{"version":"{version}"}}}}"#),
        )
        .unwrap();
        commit_and_tag(&samples, tag);
    }

    let mojang = root.join("mojang");
    std::fs::create_dir_all(&mojang).unwrap();
    let zip = mojang.join("bedrock-server-1.26.51.1.zip");
    write_zip(
        &zip,
        &[
            ("bedrock_server", b"\x7fELF fake".as_slice()),
            ("definitions/x.json", b"{}".as_slice()),
        ],
    );
    let good = Sha256Digest::of_bytes(&std::fs::read(&zip).unwrap()).to_hex();
    let zip_50 = mojang.join("bedrock-server-1.26.50.4.zip");
    write_zip(&zip_50, &[("bedrock_server", b"\x7fELF fake".as_slice())]);
    let wrong = Sha256Digest::of_bytes(b"other").to_hex();
    let index = root.join("index");
    for (version, url, sha) in [
        ("1.26.51", file_url(&zip), good.as_str()),
        ("1.26.50", file_url(&zip_50), wrong.as_str()),
        (
            "1.26.45",
            "https://mirror.invalid/b.zip".to_string(),
            good.as_str(),
        ),
    ] {
        let d = index.join("release").join(version);
        std::fs::create_dir_all(&d).unwrap();
        let entry = format!(r#"{{"binary":{{"linux":{{"url":"{url}","sha256":"{sha}"}}}}}}"#);
        std::fs::write(d.join("metadata.json"), entry).unwrap();
    }

    std::fs::write(
        root.join("links.json"),
        format!(
            r#"{{"result":{{"links":[{{"downloadType":"serverBedrockLinux","downloadUrl":"{}"}}]}}}}"#,
            file_url(&zip)
        ),
    )
    .unwrap();

    Fixture {
        root,
        remotes: |root| Remotes {
            samples: file_url(&root.join("samples")),
            docs: file_url(&root.join("docs")),
            bds_index: file_url(&root.join("index")),
            bds_links: file_url(&root.join("links.json")),
            bds_host: "file://".into(),
            curl_protocols: "=file".into(),
        },
    }
}

impl Fixture {
    /// Point the index entry for `version` (and Mojang's links API) at a new
    /// zip named `file` holding `entries`, and return the zip's path.
    fn serve_bds(&self, version: &str, file: &str, entries: &[(&str, &[u8])]) -> PathBuf {
        let dir = self.root.join("mojang").join(version);
        std::fs::create_dir_all(&dir).unwrap();
        let zip = dir.join(file);
        write_zip(&zip, entries);
        let sha = Sha256Digest::of_bytes(&std::fs::read(&zip).unwrap()).to_hex();
        let url = file_url(&zip);
        let d = self.root.join("index/release").join(version);
        std::fs::create_dir_all(&d).unwrap();
        let entry = format!(r#"{{"binary":{{"linux":{{"url":"{url}","sha256":"{sha}"}}}}}}"#);
        std::fs::write(d.join("metadata.json"), entry).unwrap();
        std::fs::write(
            self.root.join("links.json"),
            format!(
                r#"{{"result":{{"links":[{{"downloadType":"serverBedrockLinux","downloadUrl":"{url}"}}]}}}}"#
            ),
        )
        .unwrap();
        zip
    }

    fn options(&self, version: &str) -> Options {
        Options {
            game_version: version.into(),
            out: self.root.join("work"),
            eula_accepted: true,
            skip_bds: false,
            bds_zip: None,
            samples_ref: None,
            docs_ref: None,
            fetched_by: "dataminer test".into(),
            remotes: (self.remotes)(&self.root),
        }
    }
}

#[test]
fn fetches_pins_and_is_idempotent() {
    let fx = fixture("e2e");
    let report = fetch(&fx.options("1.26.51")).unwrap();
    let dir = &report.dir;
    assert_eq!(report.fetched.len(), 3);
    assert_eq!(report.sources.protocol, Some(2193));

    let docs = report.sources.get(Kind::BedrockProtocolDocs).unwrap();
    assert_eq!(docs.git_ref.as_deref(), Some("v1.26.51"));
    assert_eq!(docs.r#match, Match::Exact);

    // No samples tag for 1.26.51: the same-train tag is used, with evidence.
    let samples = report.sources.get(Kind::BedrockSamples).unwrap();
    assert_eq!(samples.git_ref.as_deref(), Some("v1.26.50.4"));
    assert_eq!(samples.game_version, "1.26.50.4");
    assert_eq!(samples.r#match, Match::SameProtocol);
    assert_eq!(samples.protocol, Some(2193));
    assert!(samples.notes[0].contains("v1.26.50"), "{:?}", samples.notes);
    assert!(dir.join("bedrock-samples/version.json").is_file());
    assert!(!dir.join("bedrock-samples/behavior_pack").exists());

    let server = report.sources.get(Kind::BedrockServer).unwrap();
    assert_eq!(server.game_version, "1.26.51.1");
    assert!(
        server.notes[0].contains("confirmed by Mojang"),
        "{:?}",
        server.notes
    );
    assert!(dir.join("bedrock-server/bedrock_server").is_file());
    assert!(!dir.join("bedrock-server.zip.partial").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join("bedrock-server/bedrock_server"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
    }

    let before = std::fs::read(dir.join("sources.json")).unwrap();
    let again = fetch(&fx.options("1.26.51")).unwrap();
    assert!(again.fetched.is_empty());
    assert_eq!(std::fs::read(dir.join("sources.json")).unwrap(), before);
    verify(dir).unwrap();
}

#[test]
fn detects_a_corrupted_cache() {
    let fx = fixture("corrupt");
    let mut opts = fx.options("1.26.51");
    opts.skip_bds = true;
    let dir = fetch(&opts).unwrap().dir;
    std::fs::write(
        dir.join("bedrock-samples/metadata/vanilladata_modules/mojang-blocks.json"),
        "tampered",
    )
    .unwrap();

    let err = format!("{:#}", verify(&dir).unwrap_err());
    assert!(err.contains("bedrock-samples has changed"), "{err}");
    let err = format!("{:#}", fetch(&opts).err().unwrap());
    assert!(err.contains("bedrock-samples has changed"), "{err}");
}

#[test]
fn refuses_a_samples_tag_with_another_protocol() {
    let fx = fixture("protocol");
    let mut opts = fx.options("1.26.45");
    opts.skip_bds = true;
    let err = format!("{:#}", fetch(&opts).err().unwrap());
    assert!(err.contains("v1.26.40.05, which is protocol 2190"), "{err}");
    assert!(err.contains("not the target's 2191"), "{err}");

    opts.samples_ref = Some("v1.26.40.05".into());
    let report = fetch(&opts).unwrap();
    let samples = report.sources.get(Kind::BedrockSamples).unwrap();
    assert_eq!(samples.r#match, Match::Explicit);
    assert_eq!(samples.protocol, None);
}

#[test]
fn bds_must_match_the_index_and_come_from_mojang() {
    let fx = fixture("bds");
    let err = format!("{:#}", fetch(&fx.options("1.26.50")).err().unwrap());
    assert!(err.contains("but the index says"), "{err}");
    assert!(!fx.root.join("work/1.26.50/bedrock-server").exists());
    assert!(
        !fx.root
            .join("work/1.26.50/bedrock-server.zip.partial")
            .exists()
    );

    let mut opts = fx.options("1.26.45");
    opts.samples_ref = Some("v1.26.40.05".into());
    let err = format!("{:#}", fetch(&opts).err().unwrap());
    assert!(err.contains("refusing to download a mirror"), "{err}");
}

#[test]
fn bds_url_must_agree_with_mojangs_links_api() {
    let fx = fixture("links");
    std::fs::write(
        fx.root.join("links.json"),
        r#"{"result":{"links":[{"downloadType":"serverBedrockLinux",
            "downloadUrl":"https://www.minecraft.net/x/bedrock-server-1.26.51.2.zip"}]}}"#,
    )
    .unwrap();
    let err = format!("{:#}", fetch(&fx.options("1.26.51")).err().unwrap());
    assert!(err.contains("Mojang's download-links API says"), "{err}");
}

#[test]
fn uses_mojangs_branch_when_a_release_is_untagged() {
    let fx = fixture("branch");
    let mut opts = fx.options("1.26.52");
    opts.skip_bds = true;
    let report = fetch(&opts).unwrap();
    let docs = report.sources.get(Kind::BedrockProtocolDocs).unwrap();
    assert_eq!(docs.git_ref.as_deref(), Some("automated/1.26.52"));
    assert_eq!(docs.game_version, "1.26.52");
    assert_eq!(docs.protocol, Some(2193));
    assert!(docs.notes[0].contains("no release tag"), "{:?}", docs.notes);
    let samples = report.sources.get(Kind::BedrockSamples).unwrap();
    assert_eq!(samples.git_ref.as_deref(), Some("v1.26.50.4"));
    assert_eq!(samples.r#match, Match::SameProtocol);
}

#[test]
fn requires_the_eula_and_a_known_tag() {
    let fx = fixture("eula");
    let mut opts = fx.options("1.26.51");
    opts.eula_accepted = false;
    let err = format!("{:#}", fetch(&opts).err().unwrap());
    assert!(err.contains("--accept-eula"), "{err}");
    assert!(!fx.root.join("work").exists());

    let err = format!("{:#}", fetch(&fx.options("1.26.60")).err().unwrap());
    assert!(
        err.contains("bedrock-protocol-docs has no tag for 1.26.60 (nearest: v1.26.51)"),
        "{err}"
    );
}

#[test]
fn rejects_non_canonical_versions() {
    let fx = fixture("canonical");
    for v in ["1.26.051", "1.26.50.4"] {
        let err = format!("{:#}", fetch(&fx.options(v)).err().unwrap());
        assert!(err.contains("is not a release version"), "{v}: {err}");
    }
}

#[test]
fn a_late_failure_does_not_wedge_the_next_run() {
    let fx = fixture("wedge");
    let mut opts = fx.options("1.26.51");
    opts.skip_bds = true;
    opts.docs_ref = Some("no-protocol".into());
    let err = format!("{:#}", fetch(&opts).err().unwrap());
    assert!(err.contains("no x-protocol-version"), "{err}");

    opts.docs_ref = None;
    fetch(&opts).unwrap();
}

#[test]
fn samples_take_no_protocol_from_an_explicit_docs_ref() {
    let fx = fixture("explicit-docs");
    let mut opts = fx.options("1.26.50");
    opts.skip_bds = true;
    opts.docs_ref = Some("v1.26.40".into());
    let report = fetch(&opts).unwrap();
    assert_eq!(report.sources.protocol, None);
    let samples = report.sources.get(Kind::BedrockSamples).unwrap();
    assert_eq!(samples.r#match, Match::Exact);
    assert_eq!(samples.protocol, None);
}

#[test]
fn verify_needs_docs_and_samples() {
    let fx = fixture("verify-partial");
    let mut opts = fx.options("1.26.45");
    opts.skip_bds = true;
    // Samples are refused (another protocol), so only docs get recorded.
    assert!(fetch(&opts).is_err());
    let err = format!("{:#}", verify(&fx.root.join("work/1.26.45")).unwrap_err());
    assert!(err.contains("bedrock-samples is not recorded"), "{err}");
}

#[test]
fn bds_must_be_the_target_version() {
    let fx = fixture("bds-version");
    fx.serve_bds(
        "1.26.40",
        "bedrock-server-1.26.51.1.zip",
        &[("bedrock_server", b"x".as_slice())],
    );
    let err = format!("{:#}", fetch(&fx.options("1.26.40")).err().unwrap());
    assert!(err.contains("is BDS 1.26.51.1, not 1.26.40"), "{err}");

    fx.serve_bds(
        "1.26.40",
        "bedrock-server",
        &[("bedrock_server", b"x".as_slice())],
    );
    let err = format!("{:#}", fetch(&fx.options("1.26.40")).err().unwrap());
    assert!(err.contains("is not a BDS download"), "{err}");
}

#[test]
fn a_blocked_links_api_stops_the_fetch() {
    let fx = fixture("links-blocked");
    std::fs::remove_file(fx.root.join("links.json")).unwrap();
    let err = format!("{:#}", fetch(&fx.options("1.26.51")).err().unwrap());
    assert!(err.contains("Mojang's download-links API"), "{err}");
    assert!(!fx.root.join("work/1.26.51/bedrock-server").exists());
}

#[test]
fn a_failed_unpack_leaves_no_debris() {
    let fx = fixture("debris");
    fx.serve_bds(
        "1.26.51",
        "bedrock-server-1.26.51.1.zip",
        &[("README.txt", b"windows build".as_slice())],
    );
    let err = format!("{:#}", fetch(&fx.options("1.26.51")).err().unwrap());
    assert!(err.contains("no bedrock_server"), "{err}");
    let left: Vec<_> = std::fs::read_dir(fx.root.join("work/1.26.51"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".partial"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn uses_and_keeps_a_user_supplied_zip() {
    let fx = fixture("bds-zip");
    let mut opts = fx.options("1.26.51");
    opts.bds_zip = Some(fx.root.join("mojang/bedrock-server-1.26.51.1.zip"));
    let report = fetch(&opts).unwrap();
    let server = report.sources.get(Kind::BedrockServer).unwrap();
    assert!(
        server
            .notes
            .iter()
            .any(|n| n.contains("supplied by the user")),
        "{:?}",
        server.notes
    );
    assert!(opts.bds_zip.as_ref().unwrap().is_file());

    let fx = fixture("bds-zip-bad");
    let bad = fx.root.join("mine.zip");
    write_zip(&bad, &[("bedrock_server", b"other".as_slice())]);
    let mut opts = fx.options("1.26.51");
    opts.bds_zip = Some(bad.clone());
    let err = format!("{:#}", fetch(&opts).err().unwrap());
    assert!(err.contains("but the index says"), "{err}");
    assert!(bad.is_file());
}

#[test]
fn downloads_keep_the_raw_bytes() {
    let dir = test_dir("raw-bytes");
    let path = dir.join("index.json");
    std::fs::write(&path, b"{\"v\":\"\xff\"}").unwrap();
    let got = bds::download(&file_url(&path), None, "=file").unwrap();
    assert_eq!(got, std::fs::read(&path).unwrap());
}

fn init(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git_in(dir, &["init", "--quiet", "--initial-branch", "main"]);
}

fn commit_and_tag(dir: &Path, tag: &str) {
    git_in(dir, &["add", "--all"]);
    git_in(dir, &["commit", "--quiet", "--message", tag]);
    git_in(dir, &["tag", tag]);
}

fn git_in(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=test", "-c", "user.email=test@invalid"])
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?} failed");
}

fn file_url(path: &Path) -> String {
    let p = path.to_string_lossy().replace('\\', "/");
    match p.strip_prefix('/') {
        Some(rest) => format!("file:///{rest}"),
        None => format!("file:///{p}"),
    }
}
