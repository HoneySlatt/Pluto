package dev.dioxus.main

// Pluto's activity (Dioxus.toml `android_main_activity`): the app draws edge
// to edge, under the status and navigation bars, and hands their heights to
// the page as the `--sat` and `--sab` CSS variables (in CSS pixels), which
// main.css adds to the screens' padding. The keyboard still shrinks the page:
// its height becomes the WebView container's bottom padding.

import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.View
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat

typealias BuildConfig = app.pluto.mobile.BuildConfig

class MainActivity : WryActivity() {
    @Volatile private var top = 0f
    @Volatile private var bottom = 0f

    override fun onCreate(savedInstanceState: Bundle?) {
        WindowCompat.setDecorFitsSystemWindows(window, false)
        @Suppress("DEPRECATION")
        window.statusBarColor = Color.TRANSPARENT
        @Suppress("DEPRECATION")
        window.navigationBarColor = Color.TRANSPARENT
        // No scrim behind the three-button bar either.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            window.isNavigationBarContrastEnforced = false
            window.isStatusBarContrastEnforced = false
        }
        super.onCreate(savedInstanceState)
    }

    /** The page reads the insets once loaded (`PlutoInsets.read()`). */
    inner class Insets {
        @JavascriptInterface
        fun read(): String = "{\"top\":$top,\"bottom\":$bottom}"
    }

    override fun onWebViewCreate(webView: WebView) {
        webView.addJavascriptInterface(Insets(), "PlutoInsets")
        ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
            val density = resources.displayMetrics.density
            val bars = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
            val keyboard = ime.bottom > bars.bottom
            top = bars.top / density
            // Over the keyboard the page ends at its top: no bar to clear.
            bottom = if (keyboard) 0f else bars.bottom / density
            (view.parent as? View)?.setPadding(0, 0, 0, if (keyboard) ime.bottom else 0)
            webView.evaluateJavascript(
                "document.documentElement.style.setProperty('--sat','${top}px');" +
                    "document.documentElement.style.setProperty('--sab','${bottom}px');",
                null,
            )
            insets
        }
        webView.post { ViewCompat.requestApplyInsets(webView) }
    }
}
