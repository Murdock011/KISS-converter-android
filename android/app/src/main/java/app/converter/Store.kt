package app.converter

import android.content.Context
import app.converter.core.FileKind
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Saved settings: "Video to MP3" and so on. */
data class Preset(
    val id: Long,
    val name: String,
    val kind: FileKind,
    val target: String,
    val quality: Int,
    val maxDimension: Int,
)

/** One converted file in the recent list. */
data class RecentItem(
    val path: String,
    val name: String,
    val fromExt: String,
    val toExt: String,
    val inBytes: Long,
    val outBytes: Long,
    val time: Long,
) {
    val file get() = File(path)
}

/** Presets and the recent list, kept in SharedPreferences as JSON. */
class Store(ctx: Context) {
    private val prefs = ctx.applicationContext.getSharedPreferences("converter", Context.MODE_PRIVATE)

    fun presets(): List<Preset> {
        val raw = prefs.getString(KEY_PRESETS, null) ?: return DEFAULT_PRESETS
        return runCatching {
            val a = JSONArray(raw)
            (0 until a.length()).map { i ->
                val o = a.getJSONObject(i)
                Preset(
                    id = o.getLong("id"),
                    name = o.getString("name"),
                    kind = FileKind.valueOf(o.getString("kind")),
                    target = o.getString("target"),
                    quality = o.getInt("quality"),
                    maxDimension = o.getInt("maxDimension"),
                )
            }
        }.getOrDefault(DEFAULT_PRESETS)
    }

    fun savePresets(list: List<Preset>) {
        val a = JSONArray()
        list.forEach {
            a.put(
                JSONObject()
                    .put("id", it.id).put("name", it.name).put("kind", it.kind.name)
                    .put("target", it.target).put("quality", it.quality).put("maxDimension", it.maxDimension),
            )
        }
        prefs.edit().putString(KEY_PRESETS, a.toString()).apply()
    }

    /** Recent items whose file still exists, newest first. */
    fun recent(): List<RecentItem> {
        val raw = prefs.getString(KEY_RECENT, null) ?: return emptyList()
        return runCatching {
            val a = JSONArray(raw)
            (0 until a.length()).map { i ->
                val o = a.getJSONObject(i)
                RecentItem(
                    path = o.getString("path"),
                    name = o.getString("name"),
                    fromExt = o.getString("fromExt"),
                    toExt = o.getString("toExt"),
                    inBytes = o.getLong("inBytes"),
                    outBytes = o.getLong("outBytes"),
                    time = o.getLong("time"),
                )
            }.filter { it.file.exists() }
        }.getOrDefault(emptyList())
    }

    fun saveRecent(list: List<RecentItem>) {
        val a = JSONArray()
        list.forEach {
            a.put(
                JSONObject()
                    .put("path", it.path).put("name", it.name).put("fromExt", it.fromExt).put("toExt", it.toExt)
                    .put("inBytes", it.inBytes).put("outBytes", it.outBytes).put("time", it.time),
            )
        }
        prefs.edit().putString(KEY_RECENT, a.toString()).apply()
    }

    companion object {
        private const val KEY_PRESETS = "presets"
        private const val KEY_RECENT = "recent"
        const val MAX_RECENT = 30

        val DEFAULT_PRESETS = listOf(
            Preset(1, "Photos to small JPG", FileKind.IMAGE, "jpg", 60, 1920),
            Preset(2, "Video to MP3", FileKind.VIDEO, "mp3", 85, 0),
            Preset(3, "Word to PDF", FileKind.DOCUMENT, "pdf", 85, 0),
        )
    }
}
