//! The remembered server: its address in the app's private files, its token
//! encrypted with an AES-GCM key the Android Keystore holds (the key never
//! leaves the Keystore; uninstalling the app deletes both). Elsewhere
//! nothing is remembered.

use zeroize::Zeroizing;

/// The remembered address and token.
pub fn load() -> Option<(String, Zeroizing<String>)> {
    imp::load()
}

/// The remembered address alone, to prefill the connection card.
pub fn load_server() -> Option<String> {
    imp::read().map(|(server, _)| server)
}

pub fn save(server: &str, token: &str) -> Result<(), String> {
    imp::save(server, token)
}

pub fn forget() {
    imp::forget();
}

/// Forgets the token but keeps the address for the connection screen.
pub fn forget_token() {
    imp::forget_token();
}

/// The app's private files, where the preferences are kept too.
pub fn files_dir() -> Option<std::path::PathBuf> {
    imp::files_dir()
}

/// Whether the phone shows a 24-hour clock (`DateFormat.is24HourFormat`);
/// `None` off Android.
pub fn clock_24h() -> Option<bool> {
    imp::clock_24h()
}

/// A sealed token: the IV and the ciphertext.
type Sealed = (Vec<u8>, Vec<u8>);

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn unhex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(target_os = "android")]
mod imp {
    use std::path::PathBuf;

    use jni::objects::{JByteArray, JObject, JValue};
    use jni::{JNIEnv, JavaVM};
    use zeroize::Zeroizing;

    const ALIAS: &str = "pluto-server-token";
    const FILE: &str = "server";

    fn with_env<T>(
        run: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<T>,
    ) -> Result<T, String> {
        let context = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let activity = unsafe { JObject::from_raw(context.context().cast()) };
        let result = run(&mut env, &activity);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        result.map_err(|e| e.to_string())
    }

    fn path() -> Option<PathBuf> {
        files_dir().map(|dir| dir.join(FILE))
    }

    pub fn clock_24h() -> Option<bool> {
        with_env(|env, activity| {
            env.call_static_method(
                "android/text/format/DateFormat",
                "is24HourFormat",
                "(Landroid/content/Context;)Z",
                &[JValue::Object(activity)],
            )?
            .z()
        })
        .ok()
    }

    /// The app's private files (`Context.getFilesDir`).
    pub fn files_dir() -> Option<PathBuf> {
        with_env(|env, activity| {
            let dir = env
                .call_method(activity, "getFilesDir", "()Ljava/io/File;", &[])?
                .l()?;
            let path = env
                .call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])?
                .l()?;
            let path: String = env.get_string(&path.into())?.into();
            Ok(PathBuf::from(path))
        })
        .ok()
    }

    /// The Keystore's key, created on first use.
    fn key<'a>(env: &mut JNIEnv<'a>) -> jni::errors::Result<JObject<'a>> {
        let provider = env.new_string("AndroidKeyStore")?;
        let store = env
            .call_static_method(
                "java/security/KeyStore",
                "getInstance",
                "(Ljava/lang/String;)Ljava/security/KeyStore;",
                &[JValue::Object(&provider)],
            )?
            .l()?;
        env.call_method(
            &store,
            "load",
            "(Ljava/security/KeyStore$LoadStoreParameter;)V",
            &[JValue::Object(&JObject::null())],
        )?;
        let alias = env.new_string(ALIAS)?;
        let present = env
            .call_method(
                &store,
                "containsAlias",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&alias)],
            )?
            .z()?;
        if !present {
            let aes = env.new_string("AES")?;
            let generator = env
                .call_static_method(
                    "javax/crypto/KeyGenerator",
                    "getInstance",
                    "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/KeyGenerator;",
                    &[JValue::Object(&aes), JValue::Object(&provider)],
                )?
                .l()?;
            // PURPOSE_ENCRYPT | PURPOSE_DECRYPT.
            let builder = env.new_object(
                "android/security/keystore/KeyGenParameterSpec$Builder",
                "(Ljava/lang/String;I)V",
                &[JValue::Object(&alias), JValue::Int(3)],
            )?;
            let strings = |env: &mut JNIEnv<'a>, value: &str| -> jni::errors::Result<JObject<'a>> {
                let item = env.new_string(value)?;
                let array = env.new_object_array(1, "java/lang/String", &item)?;
                Ok(JObject::from(array))
            };
            let modes = strings(env, "GCM")?;
            let builder = env
                .call_method(
                    &builder,
                    "setBlockModes",
                    "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
                    &[JValue::Object(&modes)],
                )?
                .l()?;
            let paddings = strings(env, "NoPadding")?;
            let builder = env
                .call_method(
                    &builder,
                    "setEncryptionPaddings",
                    "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
                    &[JValue::Object(&paddings)],
                )?
                .l()?;
            let spec = env
                .call_method(
                    &builder,
                    "build",
                    "()Landroid/security/keystore/KeyGenParameterSpec;",
                    &[],
                )?
                .l()?;
            env.call_method(
                &generator,
                "init",
                "(Ljava/security/spec/AlgorithmParameterSpec;)V",
                &[JValue::Object(&spec)],
            )?;
            env.call_method(&generator, "generateKey", "()Ljavax/crypto/SecretKey;", &[])?;
        }
        env.call_method(
            &store,
            "getKey",
            "(Ljava/lang/String;[C)Ljava/security/Key;",
            &[JValue::Object(&alias), JValue::Object(&JObject::null())],
        )?
        .l()
    }

    fn cipher<'a>(env: &mut JNIEnv<'a>) -> jni::errors::Result<JObject<'a>> {
        let name = env.new_string("AES/GCM/NoPadding")?;
        env.call_static_method(
            "javax/crypto/Cipher",
            "getInstance",
            "(Ljava/lang/String;)Ljavax/crypto/Cipher;",
            &[JValue::Object(&name)],
        )?
        .l()
    }

    /// The IV and the ciphertext of `plain`.
    fn encrypt(plain: &[u8]) -> Result<super::Sealed, String> {
        with_env(|env, _| {
            let key = key(env)?;
            let cipher = cipher(env)?;
            env.call_method(
                &cipher,
                "init",
                "(ILjava/security/Key;)V",
                &[JValue::Int(1), JValue::Object(&key)],
            )?;
            let iv = JByteArray::from(env.call_method(&cipher, "getIV", "()[B", &[])?.l()?);
            let input = env.byte_array_from_slice(plain)?;
            let sealed = JByteArray::from(
                env.call_method(&cipher, "doFinal", "([B)[B", &[JValue::Object(&input)])?
                    .l()?,
            );
            Ok((
                env.convert_byte_array(&iv)?,
                env.convert_byte_array(&sealed)?,
            ))
        })
    }

    fn decrypt(iv: &[u8], sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        with_env(|env, _| {
            let key = key(env)?;
            let cipher = cipher(env)?;
            let iv = env.byte_array_from_slice(iv)?;
            let spec = env.new_object(
                "javax/crypto/spec/GCMParameterSpec",
                "(I[B)V",
                &[JValue::Int(128), JValue::Object(&iv)],
            )?;
            env.call_method(
                &cipher,
                "init",
                "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V",
                &[JValue::Int(2), JValue::Object(&key), JValue::Object(&spec)],
            )?;
            let input = env.byte_array_from_slice(sealed)?;
            let plain = JByteArray::from(
                env.call_method(&cipher, "doFinal", "([B)[B", &[JValue::Object(&input)])?
                    .l()?,
            );
            Ok(Zeroizing::new(env.convert_byte_array(&plain)?))
        })
    }

    /// The file's address and, when present, the sealed token.
    pub fn read() -> Option<(String, Option<super::Sealed>)> {
        let text = std::fs::read_to_string(path()?).ok()?;
        let mut lines = text.lines();
        let server = lines.next()?.to_owned();
        let sealed = lines
            .next()
            .and_then(super::unhex)
            .zip(lines.next().and_then(super::unhex));
        Some((server, sealed))
    }

    pub fn load() -> Option<(String, Zeroizing<String>)> {
        let (server, sealed) = read()?;
        let (iv, sealed) = sealed?;
        let plain = decrypt(&iv, &sealed).ok()?;
        let token = Zeroizing::new(String::from_utf8(plain.to_vec()).ok()?);
        Some((server, token))
    }

    pub fn save(server: &str, token: &str) -> Result<(), String> {
        let (iv, sealed) = encrypt(token.as_bytes())?;
        let path = path().ok_or("The app's files are unavailable")?;
        let text = format!("{server}\n{}\n{}\n", super::hex(&iv), super::hex(&sealed));
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    pub fn forget() {
        if let Some(path) = path() {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Keeps the address, drops the sealed token.
    pub fn forget_token() {
        if let (Some(path), Some((server, _))) = (path(), read()) {
            let _ = std::fs::write(path, format!("{server}\n"));
        }
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use zeroize::Zeroizing;

    pub fn read() -> Option<(String, Option<super::Sealed>)> {
        None
    }

    pub fn load() -> Option<(String, Zeroizing<String>)> {
        None
    }

    pub fn save(_server: &str, _token: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn forget() {}

    pub fn forget_token() {}

    pub fn files_dir() -> Option<std::path::PathBuf> {
        None
    }

    pub fn clock_24h() -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hex_round_trips() {
        let bytes = [0u8, 1, 171, 255];
        assert_eq!(super::unhex(&super::hex(&bytes)).unwrap(), bytes);
        assert!(super::unhex("abc").is_none());
    }
}
