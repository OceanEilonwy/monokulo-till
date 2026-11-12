package com.monokulo.till

import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import java.security.MessageDigest

/**
 * Fob on this device, for Connect to Fob's same-device way (decision 318): its packages, the explicit intent's
 * action (Fob's decision 423), and the check that the Fob installed is signed as Fob's releases are.
 */
object FobApp {
    /** Fob's release, then a debug build of it (which installs beside it). */
    val packages = listOf("com.monokulo.fob", "com.monokulo.fob.debug")

    /** The action Fob's ConnectActivity takes, the request as the intent's data, for a result. */
    const val ACTION_CONNECT = "com.monokulo.fob.action.CONNECT"

    /**
     * The SHA-256 of Fob's release signing certificate, lower-case hex. Empty until the owner adds it: then any
     * installed Fob is opened, which costs little, the answer being sealed for this till's request alone and holding
     * no spend key (a Fob signed otherwise could only hand over a wallet that isn't the shop's).
     */
    private val certificates: Set<String> = emptySet()

    /** Whether [pkg] is installed and, once [certificates] lists any, signed with one of them. */
    fun trusted(context: Context, pkg: String): Boolean = runCatching {
        val pm = context.packageManager
        if (certificates.isEmpty()) {
            pm.getPackageInfo(pkg, 0)
            return@runCatching true
        }
        val signatures = if (Build.VERSION.SDK_INT >= 28) {
            val info = pm.getPackageInfo(pkg, PackageManager.GET_SIGNING_CERTIFICATES).signingInfo ?: return@runCatching false
            if (info.hasMultipleSigners()) info.apkContentsSigners.toList() else listOfNotNull(info.signingCertificateHistory.lastOrNull())
        } else {
            @Suppress("DEPRECATION")
            pm.getPackageInfo(pkg, PackageManager.GET_SIGNATURES).signatures?.toList().orEmpty()
        }
        signatures.any { digest(it.toByteArray()) in certificates }
    }.getOrDefault(false)

    private fun digest(bytes: ByteArray): String = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
}
