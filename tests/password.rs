#![cfg(all(target_os = "macos", feature = "samples"))]

use engine_pdf::edit::{InfoPatch, Lock, Permissions, Plan, Save, apply};
use engine_pdf::samples::Sample;
use engine_pdf::{Cancel, Document, Error};
use zeroize::Zeroizing;

#[test]
fn checks_open_permissions_and_explicit_password_changes() {
    let source = Document::from_bytes(&Sample::TextZh.generate().unwrap()).unwrap();
    let owner = Zeroizing::new("fixture-permissions".to_owned());
    let user = Zeroizing::new("fixture-reader".to_owned());
    let wrong = Zeroizing::new("fixture-wrong".to_owned());
    let permissions = Permissions {
        print: true,
        copy: false,
        modify: false,
        annotate: true,
    };
    let cancel = Cancel::new();
    let empty = Plan::default();
    let lock = Save {
        lock: Some(Lock {
            owner: owner.clone(),
            user: Some(user.clone()),
            permissions,
        }),
        ..Save::default()
    };
    let bytes = apply(&source, &[], &empty, &lock, &cancel).unwrap();
    assert!(matches!(Document::from_bytes(&bytes), Err(Error::Locked)));
    let error = Document::from_bytes_with_password(&bytes, &wrong)
        .err()
        .unwrap();
    assert!(matches!(error, Error::WrongPassword));
    let error = format!("{error} {error:?}");
    assert!(
        !error.contains(wrong.as_str())
            && !error.contains(owner.as_str())
            && !error.contains(user.as_str())
    );

    let reader = Document::from_bytes_with_password(&bytes, &user).unwrap();
    assert_eq!(reader.info().permissions, permissions);
    assert_eq!(
        apply(&reader, &[], &empty, &Save::default(), &cancel).unwrap(),
        bytes
    );
    let clear = Save {
        remove_password: true,
        ..Save::default()
    };
    assert!(matches!(
        apply(&reader, &[], &empty, &clear, &cancel),
        Err(Error::OwnerPasswordRequired)
    ));

    let authorized = Document::from_bytes_with_password(&bytes, &owner).unwrap();
    for page in 1..=source.page_count() {
        assert_eq!(authorized.text(page).unwrap(), source.text(page).unwrap());
    }
    let change = Plan {
        info: Some(InfoPatch {
            title: Some("换密码验收".into()),
            ..InfoPatch::default()
        }),
        ..Plan::default()
    };
    assert!(matches!(
        apply(&authorized, &[], &change, &Save::default(), &cancel),
        Err(Error::PasswordActionRequired)
    ));
    let clear_bytes = apply(&authorized, &[], &change, &clear, &cancel).unwrap();
    let unlocked = Document::from_bytes(&clear_bytes).unwrap();
    assert!(!unlocked.info().encrypted);
    assert_eq!(unlocked.info().title.as_deref(), Some("换密码验收"));
    for page in 1..=source.page_count() {
        assert_eq!(unlocked.text(page).unwrap(), source.text(page).unwrap());
    }

    let next = Zeroizing::new("fixture-next-reader".to_owned());
    let relock = Save {
        lock: Some(Lock {
            owner,
            user: Some(next.clone()),
            permissions,
        }),
        ..Save::default()
    };
    let replaced = apply(&authorized, &[], &change, &relock, &cancel).unwrap();
    assert!(matches!(
        Document::from_bytes_with_password(&replaced, &user),
        Err(Error::WrongPassword)
    ));
    let reopened = Document::from_bytes_with_password(&replaced, &next).unwrap();
    assert_eq!(reopened.info().permissions, permissions);
    assert_eq!(reopened.info().title.as_deref(), Some("换密码验收"));
}

#[test]
fn checks_bad_owner_password_on_a_readable_encrypted_document() {
    let doc = Document::from_bytes(&Sample::TextEn.generate().unwrap()).unwrap();
    let owner = Zeroizing::new("fixture-permissions-only".to_owned());
    let invalid = Save {
        lock: Some(Lock {
            owner: owner.clone(),
            user: Some(owner.clone()),
            permissions: Permissions::default(),
        }),
        ..Save::default()
    };
    assert!(matches!(
        apply(&doc, &[], &Plan::default(), &invalid, &Cancel::new()),
        Err(Error::InvalidPasswordOptions)
    ));

    let permissions = Permissions {
        copy: false,
        annotate: false,
        ..Permissions::default()
    };
    let save = Save {
        lock: Some(Lock {
            owner: owner.clone(),
            user: None,
            permissions,
        }),
        ..Save::default()
    };
    let bytes = apply(&doc, &[], &Plan::default(), &save, &Cancel::new()).unwrap();
    let reader = Document::from_bytes(&bytes).unwrap();
    assert!(reader.info().encrypted);
    assert_eq!(reader.info().permissions, permissions);
    let wrong = Zeroizing::new("fixture-incorrect-owner".to_owned());
    assert!(matches!(
        Document::from_bytes_with_password(&bytes, &wrong),
        Err(Error::WrongPassword)
    ));
    let authorized = Document::from_bytes_with_password(&bytes, &owner).unwrap();
    assert!(authorized.info().permissions.copy);
}
