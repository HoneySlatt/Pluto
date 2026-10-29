package app.pluto.mobile

// Android 17 keeps an app that targets it off the local network until the
// user allows it (`ACCESS_LOCAL_NETWORK`, shown as Nearby devices): a server
// on the home Wi-Fi (192.168.x.x, a `.local` name) is unreachable without
// it. The app asks before it connects to such an address; the connection
// screens say so when it was refused.

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalContext

object LocalNetwork {
    const val PERMISSION = "android.permission.ACCESS_LOCAL_NETWORK"

    // Whether `server` sits on the local network: a private or link-local
    // address, a `.local` name or a single-label name.
    fun isLocal(server: String): Boolean {
        val host = server.substringAfter("://").substringBefore('/').let { address ->
            if (address.startsWith("[")) address.substringAfter('[').substringBefore(']')
            else address.substringBeforeLast(':', address)
        }.lowercase()
        if (host.isEmpty() || host == "localhost") return false
        val parts = host.split('.')
        if (parts.size == 4 && parts.all { it.toIntOrNull() in 0..255 }) {
            val (a, b) = parts[0].toInt() to parts[1].toInt()
            return a == 10 || (a == 172 && b in 16..31) || (a == 192 && b == 168) || (a == 169 && b == 254)
        }
        if (host.contains(':')) return host.startsWith("fc") || host.startsWith("fd") || host.startsWith("fe80")
        return host.endsWith(".local") || !host.contains('.')
    }

    // Whether connecting to `server` needs the permission first.
    fun missing(context: Context, server: String): Boolean =
        Build.VERSION.SDK_INT >= 37 && isLocal(server) &&
            context.checkSelfPermission(PERMISSION) != PackageManager.PERMISSION_GRANTED

    // The app's settings page, where a refused permission can be allowed.
    fun openSettings(context: Context) {
        context.startActivity(
            Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }
}

// Runs `then` once the app may reach `server`: at once, or after the
// permission prompt (refused, it runs anyway and the connection says why).
fun interface LocalNetworkGate {
    fun connect(server: String, then: () -> Unit)
}

val LocalGate = staticCompositionLocalOf<LocalNetworkGate> { LocalNetworkGate { _, then -> then() } }

@Composable
fun rememberLocalNetworkGate(): LocalNetworkGate {
    val context = LocalContext.current
    val pending = remember { arrayOfNulls<() -> Unit>(1) }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        pending[0]?.invoke()
        pending[0] = null
    }
    return remember {
        LocalNetworkGate { server, then ->
            if (LocalNetwork.missing(context, server)) {
                // The prompt is already up: the last connection asked wins.
                val asking = pending[0] != null
                pending[0] = then
                if (!asking) ask.launch(LocalNetwork.PERMISSION)
            } else {
                then()
            }
        }
    }
}
