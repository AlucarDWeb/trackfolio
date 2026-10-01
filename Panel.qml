import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

Panel {
  id: root
  moduleName: "io.github.alucardweb.trackfolio"
  manageIpc: false

  property var anchorItem: null
  property var hostWidget: null
  property bool openedFromHotkey: false

  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property string fileOverride: String(setting("file", "") || "")
  readonly property string binaryOverride: String(setting("binary", "") || "")
  readonly property string bookFile: {
    if (fileOverride !== "") return fileOverride
    var fromEnv = Quickshell.env("TRACKFOLIO_FILE")
    if (fromEnv && fromEnv.length > 0) return fromEnv
    var xdg = Quickshell.env("XDG_DATA_HOME")
    var base = (xdg && xdg.length > 0) ? xdg : (Quickshell.env("HOME") + "/.local/share")
    return base + "/trackfolio/portfolio.json"
  }
  readonly property var headers: ["#", "TYPE", "NAME", "PRINCIPAL", "YIELD", "MATURITY", "DAY", "WEEK", "MONTH", "YEAR"]

  property string binary: ""
  property string barText: "trackfolio"
  property string barYield: ""
  property var kpis: ({})
  property var positions: []
  property var fx: null
  property string message: ""
  property string lastError: ""
  property int selected: -1
  property int deleteArmed: -1
  property bool primed: false
  property bool loading: false
  property bool busy: false
  property var pending: null
  property var activeJob: null
  property string outBuf: ""
  property string errBuf: ""
  property string themeGreen: ""
  property string themeRed: ""

  property bool formOpen: false
  property string formMode: "add"
  property string formKind: "tbill"
  property string formCurrency: "USD"
  property string formError: ""

  readonly property color upColor: themeGreen !== "" ? themeGreen : "#5fbf6f"
  readonly property color downColor: themeRed !== "" ? themeRed : Color.urgent
  readonly property string dateLabel: formKind === "deposit" ? "start date" : "maturity"
  readonly property bool keysBlocked: nameField.activeFocus
    || amountField.activeFocus
    || yieldField.activeFocus
    || dateField.activeFocus
    || kindDropdown.popupOpen
    || currencyDropdown.popupOpen

  function open() {
    openedFromHotkey = false
    setCenterHoverRevealSuppressed(false)
    root.controller.show()
    Qt.callLater(function() { if (keyCatcher) keyCatcher.forceActiveFocus() })
  }

  function openFromHotkey() {
    openedFromHotkey = true
    root.controller.show()
    Qt.callLater(function() {
      if (root.opened) setCenterHoverRevealSuppressed(true)
      if (keyCatcher) keyCatcher.forceActiveFocus()
    })
  }

  function close() {
    setCenterHoverRevealSuppressed(false)
    if (root.formOpen) root.cancelForm()
    root.controller.hide()
  }

  function toggle() {
    if (root.opened) root.close()
    else root.openFromHotkey()
  }

  function switchPanel(direction) {
    if (root.bar && typeof root.bar.switchPanelFrom === "function")
      return root.bar.switchPanelFrom(root.hostWidget || root, direction)
    return false
  }

  function setCenterHoverRevealSuppressed(value) {
    if (root.bar && typeof root.bar.setCenterHoverRevealSuppressed === "function")
      root.bar.setCenterHoverRevealSuppressed(value)
    else if (root.bar && "centerHoverRevealSuppressed" in root.bar)
      root.bar.centerHoverRevealSuppressed = value
  }

  function refresh() {
    root.refreshFx()
  }

  function refreshFx() {
    root.enqueue({ mode: "fx" })
  }

  function refreshBook() {
    root.enqueue({ mode: "book" })
  }

  function probeBinary() {
    if (probe.running) return
    probe.command = [
      "bash", "-c",
      "if [ -n \"$1\" ] && [ -x \"$1\" ]; then printf %s \"$1\"; exit 0; fi; " +
      "for p in \"$HOME/.cargo/bin/trackfolio\" \"$HOME/.local/bin/trackfolio\"; do " +
      "if [ -x \"$p\" ]; then printf %s \"$p\"; exit 0; fi; done; " +
      "found=$(command -v trackfolio 2>/dev/null || true); " +
      "if [ -n \"$found\" ]; then printf %s \"$found\"; exit 0; fi; exit 1",
      "bash",
      root.binaryOverride
    ]
    probe.running = true
  }

  function enqueue(job) {
    if (root.binary === "") return
    if (worker.running) {
      if (!root.pending || root.rank(job) >= root.rank(root.pending)) root.pending = job
      return
    }
    root.startJob(job)
  }

  function rank(job) {
    if (!job) return 0
    if (job.mode === "apply") return 3
    if (job.mode === "fx") return 2
    return 1
  }

  function startJob(job) {
    root.activeJob = job
    root.outBuf = ""
    root.errBuf = ""
    root.busy = true
    if (job.mode === "fx" && root.positions.length === 0) root.loading = true
    var cmd = [root.binary, "json"]
    if (job.mode === "apply") {
      cmd.push("apply")
      cmd.push("--file")
      cmd.push(root.bookFile)
      cmd.push(JSON.stringify(job.payload))
    } else {
      cmd.push("show")
      if (job.mode === "fx") cmd.push("--fx")
      cmd.push("--file")
      cmd.push(root.bookFile)
    }
    worker.command = cmd
    worker.running = true
  }

  function parseOutput(stdout, stderr, exitCode) {
    var raw = String(stdout || "").trim()
    if (raw === "") raw = String(stderr || "").trim()
    if (raw === "") return { ok: false, error: "trackfolio exited " + exitCode }
    try {
      return JSON.parse(raw)
    } catch (e) {
      return { ok: false, error: raw }
    }
  }

  function ingest(snap, mode) {
    root.loading = false
    if (!snap || snap.ok !== true) {
      var err = (snap && snap.error) ? String(snap.error) : "trackfolio failed"
      root.lastError = err
      if (mode === "apply" && root.formOpen) root.formError = err
      else root.message = err
      if (mode === "fx") root.scheduleFxRetry()
      return
    }
    root.lastError = ""
    root.kpis = snap.kpis || {}
    root.positions = snap.positions || []
    root.barText = snap.bar || root.barText
    root.barYield = snap.barYield || root.barYield
    if (mode === "fx") {
      root.fx = snap.fx
      var fxError = snap.fx && snap.fx.error ? String(snap.fx.error) : ""
      if (root.deleteArmed < 0) root.message = fxError || snap.message || ""
      if (fxError !== "") root.scheduleFxRetry()
      else root.clearFxRetry()
    } else if (mode === "apply") {
      root.message = snap.message || ""
      root.formError = ""
      root.formOpen = false
      root.deleteArmed = -1
      if (keyCatcher) keyCatcher.forceActiveFocus()
    }
    if (!root.primed) {
      root.selected = root.positions.length ? 0 : -1
      root.primed = true
    } else if (root.positions.length === 0) {
      root.selected = -1
    } else if (root.selected >= root.positions.length) {
      root.selected = root.positions.length - 1
    }
  }

  property int fxFailures: 0

  function scheduleFxRetry() {
    root.fxFailures += 1
    if (root.fxFailures <= 15) fxRetry.restart()
  }

  function clearFxRetry() {
    root.fxFailures = 0
    fxRetry.stop()
  }

  function finishJob(exitCode) {
    var job = root.activeJob
    var snap = root.parseOutput(root.outBuf, root.errBuf, exitCode)
    root.busy = false
    root.activeJob = null
    if (job) root.ingest(snap, job.mode)
    var next = root.pending
    root.pending = null
    if (next) root.startJob(next)
  }

  function moveSelection(delta) {
    var len = root.positions.length
    root.deleteArmed = -1
    root.message = ""
    if (len === 0) {
      root.selected = -1
      return
    }
    if (root.selected < 0) root.selected = 0
    else root.selected = Math.max(0, Math.min(len - 1, root.selected + delta))
  }

  function selectRow(index) {
    root.deleteArmed = -1
    root.message = ""
    root.selected = index
  }

  function openAdd() {
    if (root.busy && root.activeJob && root.activeJob.mode === "apply") return
    root.formMode = "add"
    root.formKind = "tbill"
    root.formCurrency = "USD"
    nameField.text = ""
    amountField.text = ""
    yieldField.text = ""
    dateField.text = ""
    root.formError = ""
    root.formOpen = true
    root.deleteArmed = -1
    root.message = ""
    nameField.forceActiveFocus()
  }

  function openEdit() {
    if (root.selected < 0 || root.selected >= root.positions.length) return
    var edit = root.positions[root.selected].edit || {}
    root.formMode = "edit"
    root.formKind = edit.kind || "tbill"
    root.formCurrency = edit.currency || "USD"
    nameField.text = edit.name || ""
    amountField.text = edit.amount || ""
    yieldField.text = edit["yield"] || ""
    dateField.text = edit.date || ""
    root.formError = ""
    root.formOpen = true
    root.deleteArmed = -1
    root.message = ""
    nameField.forceActiveFocus()
  }

  function cancelForm() {
    root.formOpen = false
    root.formError = ""
    if (keyCatcher) keyCatcher.forceActiveFocus()
  }

  function submitForm() {
    if (root.busy) return
    var payload = {
      op: root.formMode,
      name: nameField.text,
      kind: root.formKind,
      currency: root.formCurrency,
      amount: amountField.text,
      "yield": yieldField.text,
      date: dateField.text
    }
    if (root.formMode === "edit") payload.index = root.selected
    root.enqueue({ mode: "apply", payload: payload })
  }

  function confirmDelete() {
    if (root.formOpen || root.selected < 0 || root.selected >= root.positions.length) return
    if (root.deleteArmed === root.selected) {
      root.enqueue({ mode: "apply", payload: { op: "delete", index: root.selected } })
      root.deleteArmed = -1
      return
    }
    root.deleteArmed = root.selected
    root.message = "press d again to delete \"" + root.positions[root.selected].name + "\""
  }

  function kpiText(key) {
    if (!root.kpis || root.kpis[key] === undefined || root.kpis[key] === null) return "—"
    return String(root.kpis[key])
  }

  function fxText(key) {
    if (!root.fx || root.fx[key] === undefined || root.fx[key] === null || root.fx[key] === "") return "—"
    return String(root.fx[key])
  }

  function trendColor(trend) {
    if (trend === "up") return root.upColor
    if (trend === "down") return root.downColor
    return root.barForeground
  }

  function rowValues(row) {
    return [
      String(row.index + 1), row.kind || "", row.name || "", row.principal || "",
      row["yield"] || "", row.date || "", row.day || "", row.week || "", row.month || "", row.year || ""
    ]
  }

  function readTheme(raw) {
    root.themeGreen = root.pickTheme(raw, "green")
    root.themeRed = root.pickTheme(raw, "red")
  }

  function pickTheme(raw, key) {
    var match = String(raw || "").match(new RegExp("^" + key + "\\s*=\\s*\"([^\"]+)\"", "m"))
    return match ? match[1] : ""
  }

  Component.onCompleted: root.probeBinary()
  onBinaryOverrideChanged: root.probeBinary()
  onBinaryChanged: if (root.binary !== "") root.refreshFx()

  Timer {
    // Hourly contract. The first read is onBinaryChanged, so this does not
    // also fire at startup. A failed read retries on fxRetry instead of
    // waiting the full hour.
    interval: 60 * 60 * 1000
    repeat: true
    running: root.binary !== ""
    triggeredOnStart: false
    onTriggered: {
      root.fxFailures = 0
      root.refreshFx()
    }
  }

  Timer {
    id: fxRetry
    interval: 20000
    repeat: false
    onTriggered: root.refreshFx()
  }

  Timer {
    id: bookDebounce
    interval: 300
    onTriggered: root.refreshBook()
  }

  FileView {
    path: root.bookFile
    watchChanges: true
    printErrors: false
    onFileChanged: bookDebounce.restart()
  }

  FileView {
    path: Quickshell.env("HOME") + "/.local/state/omarchy/current/theme/colors.toml"
    watchChanges: true
    printErrors: false
    onLoaded: root.readTheme(text())
    onFileChanged: reload()
  }

  Process {
    id: probe
    running: false
    stdout: StdioCollector {
      id: probeOut
      waitForEnd: true
    }
    onExited: function(exitCode, exitStatus) {
      var found = String(probeOut.text || "").trim()
      if (exitCode === 0 && found !== "") root.binary = found
      else if (root.binary === "") {
        root.lastError = "trackfolio is not installed; cargo install --git https://github.com/AlucarDWeb/trackfolio"
        root.barText = "trackfolio"
        root.message = root.lastError
      }
    }
  }

  Process {
    id: worker
    running: false
    stdout: StdioCollector {
      id: workerOut
      waitForEnd: true
    }
    stderr: StdioCollector {
      id: workerErr
      waitForEnd: true
    }
    onExited: function(exitCode, exitStatus) {
      root.outBuf = String(workerOut.text || root.outBuf || "")
      root.errBuf = String(workerErr.text || root.errBuf || "")
      root.finishJob(exitCode)
    }
  }



  component BookCell: Text {
    property int cellWidth: Style.space(64)
    width: cellWidth
    elide: Text.ElideRight
    textFormat: Text.PlainText
    font.family: root.fontFamily
    font.pixelSize: Style.font.caption
    color: root.barForeground
  }

  component BookLine: Row {
    id: line
    property var values: []
    property bool header: false
    property bool expired: false
    spacing: Style.space(6)
    readonly property var widths: [
      Style.space(28), Style.space(68), Style.space(140), Style.space(100),
      Style.space(58), Style.space(88), Style.space(76), Style.space(76),
      Style.space(84), Style.space(88)
    ]
    readonly property var rightAligned: [false, false, false, true, true, false, true, true, true, true]

    Repeater {
      model: 10
      BookCell {
        required property int index
        cellWidth: line.widths[index]
        text: line.values.length > index ? String(line.values[index]) : ""
        horizontalAlignment: line.rightAligned[index] ? Text.AlignRight : Text.AlignLeft
        font.bold: line.header || line.expired
        color: line.header
          ? Qt.darker(root.barForeground, 1.35)
          : (line.expired ? root.downColor : root.barForeground)
      }
    }
  }

  component FieldLabel: Text {
    textFormat: Text.PlainText
    color: Qt.darker(root.barForeground, 1.35)
    font.family: root.fontFamily
    font.pixelSize: Style.font.caption
    font.bold: true
  }

  KeyboardPanel {
    id: panel
    anchorItem: root.anchorItem
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(900))
    contentHeight: panel.fittedContentHeight(content.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      blocked: root.keysBlocked
      onCloseRequested: {
        if (root.formOpen) root.cancelForm()
        else root.close()
      }
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onMoveRequested: function(dx, dy) {
        if (root.formOpen) return
        if (dy !== 0) root.moveSelection(dy)
      }
      onReturnRequested: {
        if (!root.formOpen) root.openEdit()
      }
      onTextKey: function(text) {
        if (root.formOpen) return
        if (text === "a") root.openAdd()
        else if (text === "e") root.openEdit()
        else if (text === "d") root.confirmDelete()
        else if (text === "q") root.close()
      }

      Flickable {
        anchors.fill: parent
        contentWidth: width
        contentHeight: content.implicitHeight
        clip: true

        Column {
          id: content
          width: parent.width
          spacing: Style.space(8)

          Row {
            width: parent.width
            spacing: Style.space(16)

            Row {
              spacing: Style.space(4)
              FieldLabel { text: "EURUSD" }
              Text {
                textFormat: Text.PlainText
                text: root.fxText("usd")
                color: root.barForeground
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                font.bold: true
              }
              Text {
                textFormat: Text.PlainText
                text: root.fx && root.fx.usdPct ? root.fx.usdPct : ""
                color: root.trendColor(root.fx ? root.fx.usdTrend : "")
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                font.bold: true
              }
            }
            Row {
              spacing: Style.space(4)
              FieldLabel { text: "EURYEN" }
              Text {
                textFormat: Text.PlainText
                text: root.fxText("jpy")
                color: root.barForeground
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                font.bold: true
              }
              Text {
                textFormat: Text.PlainText
                text: root.fx && root.fx.jpyPct ? root.fx.jpyPct : ""
                color: root.trendColor(root.fx ? root.fx.jpyTrend : "")
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                font.bold: true
              }
            }
            Text {
              textFormat: Text.PlainText
              text: root.fx && root.fx.date ? root.fx.date : ""
              color: root.barForeground
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              font.bold: true
            }
          }

          Flow {
            width: parent.width
            spacing: Style.space(12)
            Repeater {
              model: [
                { label: "CAPITAL", key: "capital" },
                { label: "YIELD", key: "yield" },
                { label: "DAY", key: "day" },
                { label: "WEEK", key: "week" },
                { label: "MONTH", key: "month" },
                { label: "YEAR", key: "year" }
              ]
              Row {
                required property var modelData
                spacing: Style.space(4)
                FieldLabel { text: modelData.label }
                Text {
                  textFormat: Text.PlainText
                  text: root.kpiText(modelData.key)
                  color: root.barForeground
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.caption
                  font.bold: true
                }
              }
            }
          }

          PanelSeparator { foreground: root.barForeground }

          Flickable {
            id: tableScroll
            width: parent.width
            height: root.positions.length === 0
              ? Style.space(36)
              : Math.min(headerLine.implicitHeight + tableBody.implicitHeight, Style.space(240))
            contentWidth: Math.max(width, headerLine.implicitWidth)
            contentHeight: root.positions.length === 0
              ? height
              : headerLine.implicitHeight + tableBody.implicitHeight
            clip: true
            flickableDirection: Flickable.HorizontalAndVerticalFlick

            Column {
              width: tableScroll.contentWidth
              visible: root.positions.length > 0
              BookLine { id: headerLine; header: true; values: root.headers }
              Column {
                id: tableBody
                width: parent.width
                Repeater {
                  model: root.positions
                  delegate: Item {
                    required property var modelData
                    required property int index
                    width: Math.max(headerLine.implicitWidth, tableScroll.width)
                    height: Style.space(22)

                    Rectangle {
                      anchors.fill: parent
                      visible: root.selected === index
                      color: Qt.rgba(root.barForeground.r, root.barForeground.g, root.barForeground.b, 0.14)
                    }

                    BookLine {
                      anchors.verticalCenter: parent.verticalCenter
                      values: root.rowValues(modelData)
                      expired: modelData.expired === true
                    }

                    MouseArea {
                      anchors.fill: parent
                      onClicked: root.selectRow(index)
                      onDoubleClicked: {
                        root.selectRow(index)
                        root.openEdit()
                      }
                    }
                  }
                }
              }
            }

            Text {
              anchors.centerIn: parent
              visible: root.positions.length === 0
              textFormat: Text.PlainText
              text: root.loading ? "loading…" : (root.lastError !== "" && !root.primed ? root.lastError : "press a to add")
              color: Qt.darker(root.barForeground, 1.5)
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
            }
          }

          Row {
            width: parent.width
            spacing: Style.space(8)
            Text {
              textFormat: Text.PlainText
              text: "a add  e edit  d delete  j/k move  esc close"
              color: Qt.darker(root.barForeground, 1.5)
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
            }
            Text {
              textFormat: Text.PlainText
              text: root.message
              color: Color.accent
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              font.bold: true
              elide: Text.ElideRight
              width: Math.max(0, parent.width - x)
            }
          }

          Column {
            id: form
            visible: root.formOpen
            width: parent.width
            spacing: Style.space(6)

            PanelSeparator { foreground: root.barForeground }

            Text {
              textFormat: Text.PlainText
              text: root.formMode === "edit" ? "edit position" : "add position"
              color: root.barForeground
              font.family: root.fontFamily
              font.pixelSize: Style.font.subtitle
              font.bold: true
            }

            FieldLabel { text: "name" }
            TextField {
              id: nameField
              width: parent.width
              foreground: root.barForeground
              onAccepted: root.submitForm()
              Keys.onEscapePressed: function(event) {
                root.cancelForm()
                event.accepted = true
              }
            }

            Dropdown {
              id: kindDropdown
              width: parent.width
              label: "kind"
              foreground: root.barForeground
              value: root.formKind
              options: [
                { value: "tbill", label: "T-Bill" },
                { value: "deposit", label: "Deposit" },
                { value: "other", label: "Other" }
              ]
              onChanged: function(value) { root.formKind = value }
            }

            Dropdown {
              id: currencyDropdown
              width: parent.width
              label: "currency"
              foreground: root.barForeground
              value: root.formCurrency
              options: ["USD", "EUR"]
              onChanged: function(value) { root.formCurrency = value }
            }

            FieldLabel { text: "amount" }
            TextField {
              id: amountField
              width: parent.width
              foreground: root.barForeground
              onAccepted: root.submitForm()
              Keys.onEscapePressed: function(event) {
                root.cancelForm()
                event.accepted = true
              }
            }

            FieldLabel { text: "yield %" }
            TextField {
              id: yieldField
              width: parent.width
              foreground: root.barForeground
              onAccepted: root.submitForm()
              Keys.onEscapePressed: function(event) {
                root.cancelForm()
                event.accepted = true
              }
            }

            FieldLabel { text: root.dateLabel }
            TextField {
              id: dateField
              width: parent.width
              placeholderText: "YYYY-MM-DD"
              foreground: root.barForeground
              onAccepted: root.submitForm()
              Keys.onEscapePressed: function(event) {
                root.cancelForm()
                event.accepted = true
              }
            }

            Text {
              visible: root.formError !== ""
              width: parent.width
              wrapMode: Text.WordWrap
              textFormat: Text.PlainText
              text: root.formError
              color: Color.urgent
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
            }

            Text {
              visible: root.formError === ""
              textFormat: Text.PlainText
              text: "enter: save  esc: cancel"
              color: Qt.darker(root.barForeground, 1.5)
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
            }

            Row {
              spacing: Style.space(8)
              Button {
                text: root.busy ? "saving…" : "save"
                focusable: true
                enabled: !root.busy
                onClicked: root.submitForm()
              }
              Button {
                text: "cancel"
                focusable: true
                onClicked: root.cancelForm()
              }
            }
          }
        }
      }
    }
  }
}
