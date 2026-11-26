use lopdf::{
    EncryptionState, EncryptionVersion, Object, Permissions as PdfPermissions, StringFormat,
};
use objc2::rc::autoreleasepool;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_foundation::{NSDictionary, NSMutableDictionary, NSNumber, NSString, NSUUID, ns_string};
use objc2_pdf_kit::{
    PDFAccessPermissions, PDFDocumentAccessPermissionsOption, PDFDocumentOwnerPasswordOption,
    PDFDocumentUserPasswordOption,
};

use crate::{
    Cancel, Error,
    edit::{Lock, Permissions},
};

use super::Pdf;

pub(super) fn plaintext(pdf: &Pdf) -> Result<Vec<u8>, Error> {
    if !pdf.owner_verified() {
        return Err(Error::OwnerPasswordRequired);
    }
    let bytes = autoreleasepool(|_| {
        // SAFETY: PDFKit 已验证权限密码。公开写入选项使用空打开密码去掉加密，文档只在当前线程使用。
        let data = unsafe {
            let options = NSDictionary::<NSString, NSString>::from_slices(
                &[PDFDocumentUserPasswordOption],
                &[ns_string!("")],
            );
            pdf.native()
                .dataRepresentationWithOptions(options.cast_unchecked())
        }
        .ok_or(Error::SaveFailed)?;
        Ok::<_, Error>(data.to_vec())
    })?;
    let clear = Pdf::from_bytes(&bytes).map_err(|_| Error::SaveFailed)?;
    // SAFETY: 新打开的 PDF 在当前线程确认是否仍加密。
    if unsafe { clear.native().isEncrypted() }
        || clear.page_count() != pdf.page_count()
        || clear.has_form() != pdf.has_form()
    {
        return Err(Error::SaveFailed);
    }
    Ok(bytes)
}

pub(super) fn declared_permissions(pdf: &Pdf) -> Option<Permissions> {
    let permissions = pdf.objects().ok()?.encryption_state?.permissions();
    Some(Permissions {
        print: permissions.contains(PdfPermissions::PRINTABLE),
        copy: permissions.contains(PdfPermissions::COPYABLE),
        modify: permissions.contains(PdfPermissions::MODIFIABLE),
        annotate: permissions.contains(PdfPermissions::ANNOTABLE),
    })
}

pub(super) fn encrypt(bytes: &[u8], lock: &Lock, cancel: &Cancel) -> Result<Vec<u8>, Error> {
    cancel.check()?;
    let user = lock.user.as_ref().filter(|password| !password.is_empty());
    if lock.owner.is_empty()
        || user.is_some_and(|password| password.as_str() == lock.owner.as_str())
    {
        return Err(Error::InvalidPasswordOptions);
    }

    let doc = Pdf::from_bytes(bytes)?;
    let mut access = PDFAccessPermissions::AllowsContentAccessibility.0;
    if lock.permissions.print {
        access |= PDFAccessPermissions::AllowsLowQualityPrinting.0
            | PDFAccessPermissions::AllowsHighQualityPrinting.0;
    }
    if lock.permissions.copy {
        access |= PDFAccessPermissions::AllowsContentCopying.0;
    }
    if lock.permissions.modify {
        access |= PDFAccessPermissions::AllowsDocumentChanges.0
            | PDFAccessPermissions::AllowsDocumentAssembly.0
            | PDFAccessPermissions::AllowsFormFieldEntry.0;
    }
    if lock.permissions.annotate {
        access |= PDFAccessPermissions::AllowsCommenting.0;
    }
    let encrypted = autoreleasepool(|_| {
        let owner = NSString::from_str(&lock.owner);
        let flags = NSNumber::numberWithUnsignedInteger(access);
        let options = NSMutableDictionary::<NSString, AnyObject>::new();
        // SAFETY: 密码和权限键接受 NSString / NSNumber。选项和 PDF 在当前线程使用。
        unsafe {
            options.setObject_forKey(
                &owner,
                ProtocolObject::from_ref(PDFDocumentOwnerPasswordOption),
            );
            options.setObject_forKey(
                &flags,
                ProtocolObject::from_ref(PDFDocumentAccessPermissionsOption),
            );
            if let Some(password) = user {
                let user = NSString::from_str(password);
                options.setObject_forKey(
                    &user,
                    ProtocolObject::from_ref(PDFDocumentUserPasswordOption),
                );
            }
            let options: &NSDictionary<NSString, AnyObject> = &options;
            doc.native()
                .dataRepresentationWithOptions(options.cast_unchecked())
        }
        .ok_or(Error::SaveFailed)
    })?;
    cancel.check()?;
    let encrypted = encrypted.to_vec();
    if verify(&encrypted, lock, doc.page_count())? == lock.permissions {
        return Ok(encrypted);
    }
    // PDFKit 会在只允许修改时附带批注权限。需要时用相同的加密规范写精确位值。
    let encrypted = encrypt_exact(bytes, lock, cancel)?;
    if verify(&encrypted, lock, doc.page_count())? != lock.permissions {
        return Err(Error::SaveFailed);
    }
    cancel.check()?;
    Ok(encrypted)
}

fn verify(bytes: &[u8], lock: &Lock, page_count: u32) -> Result<Permissions, Error> {
    let owner = Pdf::from_bytes_with_password(bytes, &lock.owner).map_err(|_| Error::SaveFailed)?;
    if !owner.owner_verified() || owner.page_count() != page_count {
        return Err(Error::SaveFailed);
    }
    let reader = if let Some(password) = lock.user.as_ref().filter(|password| !password.is_empty())
    {
        if !matches!(Pdf::from_bytes(bytes), Err(Error::Locked)) {
            return Err(Error::SaveFailed);
        }
        Pdf::from_bytes_with_password(bytes, password)
    } else {
        Pdf::from_bytes(bytes)
    }
    .map_err(|_| Error::SaveFailed)?;
    if reader.owner_verified() {
        return Err(Error::InvalidPasswordOptions);
    }
    declared_permissions(&reader).ok_or(Error::SaveFailed)
}

fn encrypt_exact(bytes: &[u8], lock: &Lock, cancel: &Cancel) -> Result<Vec<u8>, Error> {
    let mut pdf = super::objects::load_plain(bytes)?;
    if pdf
        .trailer
        .get(b"ID")
        .and_then(Object::as_array)
        .ok()
        .and_then(|ids| ids.first())
        .and_then(|id| id.as_str().ok())
        .is_none_or(<[u8]>::is_empty)
    {
        let id = autoreleasepool(|_| NSUUID::UUID().UUIDString().to_string().into_bytes());
        let id = Object::String(id, StringFormat::Hexadecimal);
        pdf.trailer.set(b"ID", Object::Array(vec![id.clone(), id]));
    }
    if pdf.version.as_str() < "1.4" {
        pdf.version = "1.4".into();
    }
    let mut permissions = PdfPermissions::COPYABLE_FOR_ACCESSIBILITY;
    permissions.set(
        PdfPermissions::PRINTABLE | PdfPermissions::PRINTABLE_IN_HIGH_QUALITY,
        lock.permissions.print,
    );
    permissions.set(PdfPermissions::COPYABLE, lock.permissions.copy);
    permissions.set(
        PdfPermissions::MODIFIABLE | PdfPermissions::ASSEMBLABLE | PdfPermissions::FILLABLE,
        lock.permissions.modify,
    );
    permissions.set(PdfPermissions::ANNOTABLE, lock.permissions.annotate);
    let encryption = EncryptionState::try_from(EncryptionVersion::V2 {
        document: &pdf,
        owner_password: &lock.owner,
        user_password: lock.user.as_ref().map_or("", |password| password.as_str()),
        key_length: 128,
        permissions,
    })
    .map_err(|_| Error::InvalidPasswordOptions)?;
    cancel.check()?;
    pdf.encrypt(&encryption).map_err(|_| Error::SaveFailed)?;
    super::objects::save(&mut pdf, cancel)
}
