package ws.hunke.palmguard

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class MainModel : ViewModel() {
    var state by mutableStateOf<Daemon.State?>(null)
        private set
    /** Settings as saved in the config file. */
    var saved by mutableStateOf<Settings?>(null)
        private set
    /** Settings as edited on screen, not yet saved. */
    var edited by mutableStateOf(Settings())
    var busy by mutableStateOf(false)
        private set
    var message by mutableStateOf<String?>(null)
    var log by mutableStateOf<List<String>>(emptyList())
        private set

    private var poller: Job? = null

    /** Polls daemon state while the screen is visible. */
    fun resume() {
        if (poller?.isActive == true) return
        poller = viewModelScope.launch {
            if (saved == null) loadSettings()
            while (isActive) {
                refresh()
                delay(1000)
            }
        }
    }

    fun pause() {
        poller?.cancel()
        poller = null
    }

    private suspend fun refresh() {
        state = io { Daemon.state() }
    }

    private suspend fun loadSettings() {
        val s = io { if (Daemon.state().problem == null) Daemon.loadSettings() else null } ?: return
        saved = s
        edited = s
    }

    fun setEnabled(on: Boolean) = act(if (on) "Started" else "Stopped") {
        if (on) Daemon.start() else Daemon.stop()
    }

    fun restart() = act("Restarted") { Daemon.restart() }

    fun save() = act("Saved and applied") {
        Daemon.saveSettings(edited).also { if (it.ok) saved = edited }
    }

    fun revert() {
        saved?.let { edited = it }
    }

    fun defaults() {
        edited = Settings()
    }

    fun loadLog() = viewModelScope.launch { log = io { Daemon.log() } }

    fun retry() = viewModelScope.launch {
        loadSettings()
        refresh()
    }

    private fun act(done: String, block: () -> Root.Result) = viewModelScope.launch {
        busy = true
        val r = io(block)
        message = if (r.ok) done else "Failed: ${r.text.ifBlank { "exit ${r.code}" }}"
        refresh()
        busy = false
    }

    private suspend fun <T> io(block: () -> T): T = withContext(Dispatchers.IO) { block() }
}
