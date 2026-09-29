package ws.hunke.palmguard

import java.io.BufferedReader
import java.io.BufferedWriter
import java.io.IOException

/**
 * One long-lived `su` shell, so polling the daemon doesn't fork a new root
 * process (and possibly a new Magisk prompt) every second. Blocking: call
 * from a background thread.
 */
object Root {
    class Result(val code: Int, val lines: List<String>) {
        val ok get() = code == 0
        val text get() = lines.joinToString("\n")
    }

    private var proc: Process? = null
    private var input: BufferedWriter? = null
    private var output: BufferedReader? = null
    private var seq = 0

    @Synchronized
    fun run(cmd: String): Result = try {
        runLocked(cmd)
    } catch (e: IOException) {
        close()
        Result(-1, listOf(e.message ?: "su failed"))
    }

    /** True if su works (prompts for root the first time). */
    fun available(): Boolean = run("id -u").let { it.ok && it.lines.firstOrNull() == "0" }

    private fun runLocked(cmd: String): Result {
        if (proc?.isAlive != true) {
            close()
            val p = ProcessBuilder("su").redirectErrorStream(true).start()
            proc = p
            input = p.outputStream.bufferedWriter()
            output = p.inputStream.bufferedReader()
        }
        val w = input!!
        val r = output!!
        val marker = "__palmguard_done_${++seq}__"
        // Subshell so an `exit` in cmd can't end the root shell. The leading
        // \n ends any unterminated last line of output.
        w.write("(\n$cmd\n)\nprintf '\\n%s %d\\n' $marker $?\n")
        w.flush()
        val lines = ArrayList<String>()
        while (true) {
            val line = r.readLine() ?: throw IOException("root shell closed (root denied?)")
            if (line.startsWith("$marker ")) {
                while (lines.isNotEmpty() && lines.last().isEmpty()) lines.removeAt(lines.size - 1)
                return Result(line.substringAfter(' ').trim().toIntOrNull() ?: -1, lines)
            }
            lines.add(line)
        }
    }

    private fun close() {
        runCatching { input?.close() }
        proc?.destroy()
        proc = null
        input = null
        output = null
    }
}
