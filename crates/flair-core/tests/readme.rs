//! The README settings table is generated from the registry. This test fails
//! when it is out of date; run `UPDATE_README=1 cargo test -p flair-core --test readme`
//! to regenerate it.

use flair_core::i18n::Lang;
use flair_core::settings::generate::settings_markdown;

const START: &str = "<!-- settings:start -->";
const END: &str = "<!-- settings:end -->";

#[test]
fn readme_settings_table_is_up_to_date() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md");
    // Line endings may be CRLF on a Windows checkout: compare text only.
    let readme = std::fs::read_to_string(&path)
        .expect("README.md is readable")
        .replace("\r\n", "\n");
    let (Some(start), Some(end)) = (readme.find(START), readme.find(END)) else {
        panic!("README.md must contain {START} and {END}");
    };
    let expected = format!("{START}\n{}{END}", settings_markdown(Lang::Fr));
    let current = &readme[start..end + END.len()];
    if current == expected {
        return;
    }
    if std::env::var_os("UPDATE_README").is_some() {
        let updated = format!(
            "{}{expected}{}",
            &readme[..start],
            &readme[end + END.len()..]
        );
        std::fs::write(&path, updated).expect("README.md is writable");
    } else {
        panic!(
            "README settings table is out of date: run UPDATE_README=1 cargo test -p flair-core --test readme"
        );
    }
}
