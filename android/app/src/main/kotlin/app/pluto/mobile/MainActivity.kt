package app.pluto.mobile

// Pluto's activity: the app draws edge to edge, under transparent status
// and navigation bars, and the keyboard resizes it. Coming back to the
// foreground checks the session (it may have died while the phone slept).

import android.graphics.Color
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.core.view.WindowCompat
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private lateinit var model: Model

    override fun onCreate(savedInstanceState: Bundle?) {
        installSplashScreen()
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
        )
        super.onCreate(savedInstanceState)
        // No scrim behind the three-button bar either, and light icons on
        // the dark app.
        @Suppress("DEPRECATION")
        window.navigationBarColor = Color.TRANSPARENT
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            window.isNavigationBarContrastEnforced = false
            window.isStatusBarContrastEnforced = false
        }
        WindowCompat.getInsetsController(window, window.decorView).apply {
            isAppearanceLightNavigationBars = false
            isAppearanceLightStatusBars = false
        }
        model = (application as PlutoApplication).model
        setContent { PlutoApp(model) }
        // A debug build opens a demo session (`--es demo <file in files/>`)
        // to look at the screens without a server.
        if (BuildConfig.DEBUG) {
            intent.getStringExtra("demo")?.let { name ->
                val text = java.io.File(filesDir, name).readText()
                // A remembered server connects first: the demo's images
                // then come from it.
                lifecycleScope.launch {
                    if (model.secret.load() != null) {
                        repeat(50) { if (!model.core.online()) kotlinx.coroutines.delay(200) }
                    }
                    model.core.loadDemo(text)?.let { model.core.toast(it) }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        model.core.probe()
    }
}
