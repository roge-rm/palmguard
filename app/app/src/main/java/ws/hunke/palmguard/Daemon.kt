package ws.hunke.palmguard

import android.util.Base64

/** Talks to the Magisk module (ctl.sh, config, status and log files) as root. */
object Daemon {
    const val MOD = "/data/adb/modules/palmguard"
    const val DATA = "/data/adb/palmguard"
    const val CONF = "$DATA/palmguard.conf"
    const val LOG = "$DATA/palmguard.log"
    private const val CTL = "sh $MOD/ctl.sh"

    enum class Problem { NO_ROOT, NO_MODULE }

    data class State(
        val problem: Problem? = null,
        val running: Boolean = false,
        val enabled: Boolean = false,
        val gaveUp: Boolean = false,
        /** Contents of the daemon's status file (see Status in main.rs). */
        val status: Map<String, String> = emptyMap(),
    ) {
        val filtering get() = running && status["filtering"] == "1"
        val penOut get() = status["pen_out"] == "1"
        fun num(key: String) = status[key]?.toLongOrNull() ?: 0
    }

    fun state(): State {
        val r = Root.run("[ -f $MOD/ctl.sh ] || exit 3; $CTL state; echo ---; cat $DATA/status 2>/dev/null; exit 0")
        if (r.code == 3) return State(problem = Problem.NO_MODULE)
        if (!r.ok) return State(problem = Problem.NO_ROOT)
        val split = r.lines.indexOf("---").let { if (it < 0) r.lines.size else it }
        val ctl = parse(r.lines.subList(0, split))
        return State(
            running = ctl["running"] == "1",
            enabled = ctl["enabled"] == "1",
            gaveUp = ctl["gave_up"] == "1",
            status = parse(r.lines.drop(split + 1)),
        )
    }

    fun start() = Root.run("$CTL start")
    fun stop() = Root.run("$CTL stop")
    fun restart() = Root.run("$CTL restart")
    fun toggle() = Root.run("$CTL toggle")

    fun loadSettings(): Settings = Settings.from(parse(Root.run("cat $CONF 2>/dev/null").lines))

    /** Rewrites the changed keys in place (keeping comments), then reloads. */
    fun saveSettings(s: Settings): Root.Result {
        val old = Root.run("cat $CONF 2>/dev/null").lines
        val text = Settings.merge(old, s.toMap()).joinToString("\n", postfix = "\n")
        val b64 = Base64.encodeToString(text.toByteArray(), Base64.NO_WRAP)
        return Root.run(
            "mkdir -p $DATA && echo '$b64' | base64 -d > $CONF.tmp && mv -f $CONF.tmp $CONF && $CTL reload"
        )
    }

    fun log(lines: Int = 500): List<String> =
        Root.run("tail -n $lines $LOG 2>/dev/null || echo 'No log yet.'").lines

    /** key = value lines, '#' comments ignored (same rules as the daemon). */
    fun parse(lines: List<String>): Map<String, String> = buildMap {
        for (raw in lines) {
            val line = raw.substringBefore('#').trim()
            val i = line.indexOf('=')
            if (i > 0) put(line.substring(0, i).trim(), line.substring(i + 1).trim())
        }
    }
}
