package app.pluto.mobile

// The interface's handle on the core: the Rust session (crates/pluto-ffi)
// tells it something changed, and every screen reading `revision` reads
// its data again. Changes coalesce to one per frame on the main thread.

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.text.format.DateFormat
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.pluto.core.Core
import app.pluto.core.Listener
import java.util.concurrent.atomic.AtomicBoolean

class Model(context: Context) {
    var revision by mutableIntStateOf(0)
        private set

    private val main = Handler(Looper.getMainLooper())
    private val posted = AtomicBoolean(false)

    val core: Core = Core(
        context.filesDir.absolutePath,
        DateFormat.is24HourFormat(context),
        object : Listener {
            override fun changed() {
                if (posted.compareAndSet(false, true)) {
                    main.post {
                        posted.set(false)
                        revision++
                    }
                }
            }
        },
    )

    val secret = Secret(context)
    val nav = Nav(this)

    // The composers' texts by key (`chat`, `code` or a thread id), kept
    // while the app runs.
    val texts = mutableStateMapOf<String, String>()

    // The image shown full screen, over everything.
    var viewer by mutableStateOf<androidx.compose.ui.graphics.ImageBitmap?>(null)
}
