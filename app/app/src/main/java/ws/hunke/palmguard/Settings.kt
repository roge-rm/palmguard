package ws.hunke.palmguard

/** The tunable part of palmguard.conf. Defaults match filter.rs. */
data class Settings(
    val penMaxMajor: Int = 80,
    val penRegradeMajor: Int = 150,
    val graceMs: Int = 500,
    val holdoffMs: Int = 40,
    val requirePenOut: Boolean = true,
    val verbose: Boolean = false,
) {
    val error: String?
        get() = if (penRegradeMajor <= penMaxMajor)
            "\"Pen grows into hand\" must be bigger than \"Largest pen contact\"" else null

    fun toMap() = linkedMapOf(
        "pen_max_major" to penMaxMajor.toString(),
        "pen_regrade_major" to penRegradeMajor.toString(),
        "grace_ms" to graceMs.toString(),
        "holdoff_ms" to holdoffMs.toString(),
        "require_pen_out" to requirePenOut.toString(),
        "verbose" to verbose.toString(),
    )

    companion object {
        fun from(m: Map<String, String>): Settings {
            val d = Settings()
            fun int(k: String, def: Int) = m[k]?.toIntOrNull() ?: def
            fun bool(k: String, def: Boolean) = m[k]?.let { it in setOf("1", "true", "yes") } ?: def
            return Settings(
                penMaxMajor = int("pen_max_major", d.penMaxMajor),
                penRegradeMajor = int("pen_regrade_major", d.penRegradeMajor),
                graceMs = int("grace_ms", d.graceMs),
                holdoffMs = int("holdoff_ms", d.holdoffMs),
                requirePenOut = bool("require_pen_out", d.requirePenOut),
                verbose = bool("verbose", d.verbose),
            )
        }

        /** Replace values of existing keys in [lines], append keys that are missing. */
        fun merge(lines: List<String>, values: Map<String, String>): List<String> {
            val left = LinkedHashMap(values)
            val out = lines.map { line ->
                val key = line.substringBefore('#').substringBefore('=').trim()
                val v = left.remove(key)
                if (v != null && line.substringBefore('#').contains('=')) "$key = $v" else line
            }.toMutableList()
            for ((k, v) in left) out += "$k = $v"
            return out
        }
    }
}
