# UniFFI calls the core through JNA, which reflects on these classes.
-keep class com.sun.jna.** { *; }
-keep class * extends com.sun.jna.** { *; }
-keep class app.pluto.core.** { *; }
-dontwarn java.awt.**
