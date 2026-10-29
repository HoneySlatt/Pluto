package app.pluto.mobile

// The session outlives the activity (a rotation, a theme change): the
// model lives with the process.

import android.app.Application

class PlutoApplication : Application() {
    val model: Model by lazy { Model(this) }
}
