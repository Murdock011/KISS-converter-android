# JNA and the UniFFI bindings use reflection.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class app.converter.core.** { *; }
-dontwarn java.awt.**
