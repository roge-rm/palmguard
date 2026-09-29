package ws.hunke.palmguard

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt

class MainActivity : ComponentActivity() {
    private val model: MainModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent { AppTheme { App(model) } }
    }

    override fun onResume() {
        super.onResume()
        model.resume()
    }

    override fun onPause() {
        super.onPause()
        model.pause()
    }
}

@Composable
private fun AppTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val ctx = LocalContext.current
    val colors = if (dark) dynamicDarkColorScheme(ctx) else dynamicLightColorScheme(ctx)
    MaterialTheme(colorScheme = colors, content = content)
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun App(model: MainModel) {
    var showLog by rememberSaveable { mutableStateOf(false) }
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(model.message) {
        model.message?.let {
            snackbar.showSnackbar(it)
            model.message = null
        }
    }
    BackHandler(showLog) { showLog = false }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (showLog) "PalmGuard log" else "PalmGuard") },
                navigationIcon = {
                    if (showLog) TextButton(onClick = { showLog = false }) { Text("Back") }
                },
                actions = {
                    if (showLog) TextButton(onClick = { model.loadLog() }) { Text("Refresh") }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { pad ->
        val mod = Modifier.padding(pad).fillMaxSize()
        if (showLog) {
            LaunchedEffect(Unit) { model.loadLog() }
            LogScreen(model.log, mod)
        } else {
            MainScreen(model, onShowLog = { showLog = true }, modifier = mod)
        }
    }
}

@Composable
private fun MainScreen(model: MainModel, onShowLog: () -> Unit, modifier: Modifier) {
    val state = model.state
    Column(
        modifier.verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        when {
            state == null -> Text("Connecting…")
            state.problem == Daemon.Problem.NO_ROOT -> ProblemCard(
                "Root access needed",
                "PalmGuard needs root to control its daemon. Grant it in the Magisk prompt, " +
                    "or under Magisk → Superuser, then retry.",
                model::retry,
            )
            state.problem == Daemon.Problem.NO_MODULE -> ProblemCard(
                "Magisk module not found",
                "Expected ${Daemon.MOD}. Install palmguard-magisk.zip in Magisk and reboot.",
                model::retry,
            )
            else -> {
                StatusCard(state, model)
                if (state.running) LiveCard(state, model.edited)
                SettingsCard(model)
                OutlinedButton(onClick = onShowLog, modifier = Modifier.fillMaxWidth()) { Text("View log") }
            }
        }
    }
}

@Composable
private fun ProblemCard(title: String, body: String, onRetry: () -> Unit) {
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(body)
            Button(onClick = onRetry) { Text("Retry") }
        }
    }
}

@Composable
private fun StatusCard(state: Daemon.State, model: MainModel) {
    val (headline, detail) = when {
        state.gaveUp -> "Crashed" to "The daemon exited 5 times in a row. Check the log, then switch it on again."
        !state.enabled -> "Off" to "Stock touchscreen, no filtering."
        !state.running -> "Starting…" to "Waiting for the daemon."
        state.filtering -> "Filtering" to "Pen is out: palm rejection is active."
        state.penOut -> "Ready" to "Waiting for your hand to lift before taking over the screen."
        else -> "Standby" to "Pen is docked: stock touchscreen. Pull the pen out to start filtering."
    }
    Card {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(headline, style = MaterialTheme.typography.headlineSmall)
                    Text(detail, style = MaterialTheme.typography.bodyMedium)
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = state.enabled && !state.gaveUp,
                    onCheckedChange = model::setEnabled,
                    enabled = !model.busy,
                )
            }
            if (state.enabled && !state.gaveUp) {
                TextButton(onClick = model::restart, enabled = !model.busy) { Text("Restart daemon") }
            }
        }
    }
}

@Composable
private fun LiveCard(state: Daemon.State, edited: Settings) {
    Card {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("Contact sizes since start", style = MaterialTheme.typography.titleMedium)
            Text(
                "Touch the screen with the pen, then your finger and hand, to see what size each " +
                    "one reports. \"Largest pen contact\" should sit between the biggest pen and " +
                    "smallest hand/finger size.",
                style = MaterialTheme.typography.bodySmall,
            )
            val maxPen = state.num("max_pen_major")
            val minTouch = state.num("min_touch_major")
            Stat("Pen", "last ${state.num("last_pen_major")}, largest $maxPen", "${state.num("pens")} strokes")
            Stat(
                "Finger / hand",
                "last ${state.num("last_touch_major")}, smallest $minTouch",
                "${state.num("passed")} passed, ${state.num("rejected")} blocked",
            )
            if (minTouch in 1..edited.penMaxMajor.toLong()) {
                Text(
                    "A non-pen contact as small as $minTouch was seen. At the current limit " +
                        "(${edited.penMaxMajor}) a contact that size would count as the pen.",
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}

@Composable
private fun Stat(label: String, value: String, extra: String) {
    Row {
        Text(label, Modifier.weight(0.35f), style = MaterialTheme.typography.labelLarge)
        Column(Modifier.weight(0.65f)) {
            Text(value)
            Text(extra, style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun SettingsCard(model: MainModel) {
    val s = model.edited
    val saved = model.saved
    Card {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Settings", style = MaterialTheme.typography.titleMedium)
            IntSlider(
                "Largest pen contact", "Contacts this size or smaller when they land are the pen. " +
                    "Your pen measured 30–60; hands start around 100.",
                s.penMaxMajor, 10..300, 5,
            ) { model.edited = s.copy(penMaxMajor = it) }
            IntSlider(
                "Pen grows into hand", "A pen contact that grows past this is cancelled as a hand.",
                s.penRegradeMajor, 50..800, 10,
            ) { model.edited = s.copy(penRegradeMajor = it) }
            IntSlider(
                "Grace after pen lifts", "Ignore new touches this long after the pen lifts (ms).",
                s.graceMs, 0..2000, 50,
            ) { model.edited = s.copy(graceMs = it) }
            IntSlider(
                "Touch hold-off", "Delay finger touches this long so a hand landing just before " +
                    "the pen never reaches the app (ms). Adds latency to fingers while the pen is out.",
                s.holdoffMs, 0..200, 5,
            ) { model.edited = s.copy(holdoffMs = it) }
            BoolRow(
                "Only while pen is out", "Off filters all the time, even with the pen docked.",
                s.requirePenOut,
            ) { model.edited = s.copy(requirePenOut = it) }
            BoolRow("Verbose log", "Log every contact decision.", s.verbose) {
                model.edited = s.copy(verbose = it)
            }
            s.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Button(
                    onClick = model::save,
                    enabled = !model.busy && s != saved && s.error == null,
                ) { Text("Save & apply") }
                TextButton(onClick = model::revert, enabled = s != saved) { Text("Undo") }
                TextButton(onClick = model::defaults, enabled = s != Settings()) { Text("Defaults") }
            }
        }
    }
}

@Composable
private fun IntSlider(
    label: String, help: String, value: Int, range: IntRange, step: Int, onChange: (Int) -> Unit,
) {
    Column(Modifier.padding(top = 8.dp)) {
        Row {
            Text(label, Modifier.weight(1f), style = MaterialTheme.typography.labelLarge)
            Text(value.toString(), style = MaterialTheme.typography.labelLarge)
        }
        Text(help, style = MaterialTheme.typography.bodySmall)
        Slider(
            value = value.coerceIn(range).toFloat(),
            onValueChange = { v ->
                val snapped = range.first + ((v - range.first) / step).roundToInt() * step
                if (snapped != value) onChange(snapped.coerceIn(range))
            },
            valueRange = range.first.toFloat()..range.last.toFloat(),
        )
    }
}

@Composable
private fun BoolRow(label: String, help: String, value: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.labelLarge)
            Text(help, style = MaterialTheme.typography.bodySmall)
        }
        Switch(checked = value, onCheckedChange = onChange)
    }
}

@Composable
private fun LogScreen(lines: List<String>, modifier: Modifier) {
    val list = rememberLazyListState()
    LaunchedEffect(lines.size) { if (lines.isNotEmpty()) list.scrollToItem(lines.size - 1) }
    SelectionContainer(modifier) {
        LazyColumn(
            Modifier.fillMaxSize().horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp),
            state = list,
        ) {
            items(lines) { Text(it, fontFamily = FontFamily.Monospace, fontSize = 11.sp, softWrap = false) }
        }
    }
}
