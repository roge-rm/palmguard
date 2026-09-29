package ws.hunke.palmguard

import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

/** Quick Settings tile: tap to switch PalmGuard on or off. */
class ToggleTile : TileService() {
    override fun onStartListening() = update(null)

    override fun onClick() = update { Daemon.toggle() }

    /** Runs [action] (if any) as root off the main thread, then redraws the tile. */
    private fun update(action: (() -> Unit)?) {
        Thread {
            action?.invoke()
            val s = Daemon.state()
            val tile = qsTile ?: return@Thread
            tile.state = when {
                s.problem != null -> Tile.STATE_UNAVAILABLE
                s.enabled && !s.gaveUp -> Tile.STATE_ACTIVE
                else -> Tile.STATE_INACTIVE
            }
            tile.subtitle = when {
                s.problem == Daemon.Problem.NO_ROOT -> "No root"
                s.problem == Daemon.Problem.NO_MODULE -> "Not installed"
                s.gaveUp -> "Crashed"
                !s.enabled -> "Off"
                s.filtering -> "Filtering"
                else -> "Standby"
            }
            tile.updateTile()
        }.start()
    }
}
