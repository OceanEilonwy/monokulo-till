//! The one library the apps load (docs/ios-plan.md §2): the engine with its
//! bridge turned on, and orf-codec's bindings. Besides that, Android's
//! start-up call for certificate checks, and the check that SQLCipher is
//! linked (`sqlcipher_check`, its own bindings `till_ffi`).

till_app::uniffi_reexport_scaffolding!();
orf_codec_ffi::uniffi_reexport_scaffolding!();
uniffi::setup_scaffolding!();

mod sqlcipher;

/// What Android calls once, before the engine's first request: reqwest's
/// rustls checks certificates through `rustls-platform-verifier`, which on
/// Android asks the system's trust manager through the JVM, and panics on
/// a request made before it has the app's context (decision 36). The Kotlin
/// side is `com.monokulo.till.ffi.Tls.init(context)`, with the
/// `rustls-platform-verifier` Maven component beside it.
#[cfg(target_os = "android")]
mod android {
    use jni::EnvUnowned;
    use jni::errors::ThrowRuntimeExAndDefault;
    use jni::objects::{JClass, JObject};

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_monokulo_till_ffi_Tls_initWithContext<'caller>(
        mut env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        context: JObject<'caller>,
    ) {
        env.with_env(|env| rustls_platform_verifier::android::init_with_env(env, context))
            .resolve::<ThrowRuntimeExAndDefault>();
    }
}
