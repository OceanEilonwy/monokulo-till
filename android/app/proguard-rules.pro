# UniFFI's bindings call into the native library through JNA, by reflection.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
-dontwarn java.awt.**
# rustls-platform-verifier calls its Kotlin half through JNI, which R8 cannot see (docs/decisions/core.md 36).
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
# Tls.initWithContext is bound by name to till-ffi's JNI entry.
-keepclasseswithmembernames class com.monokulo.till.ffi.Tls { native <methods>; }
