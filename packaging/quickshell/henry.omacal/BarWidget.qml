import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

BarWidget {
  id: root
  moduleName: "henry.omacal"

  property string displayText: ""
  property string tooltipText: "Omacal — click to open calendar"

  function refresh() {
    if (!queryProc.running) queryProc.running = true
  }

  implicitWidth: visible ? Math.max(48, label.implicitWidth + Style.spacing.controlPaddingX * 2) : 0
  implicitHeight: barSize
  visible: displayText !== ""

  // Allow manual IPC refresh: omarchy-shell bar broadcast henry.omacal refresh
  IpcHandler {
    target: "henry.omacal"
    function refresh(): void { root.refresh() }
  }

  Process {
    id: queryProc
    // Poll the local Omacal SQLite store for the next visible event.
    // Mirrors packaging/omacal-waybar logic but via sqlite3 directly.
    command: [
      "bash", "-c",
      "DB=\"${XDG_DATA_HOME:-$HOME/.local/share}/omacal/omacal.db\"; " +
      "if [ ! -f \"$DB\" ]; then echo -n ''; exit 0; fi; " +
      "sqlite3 -noheader -separator '|' \"$DB\" " +
      "\"SELECT summary, dtstart, location FROM objects JOIN calendars ON objects.calendar_id = calendars.id " +
      "WHERE calendars.visible = 1 AND (calendars.subscribed IS NULL OR calendars.subscribed = 1) AND objects.dtstart >= datetime('now') " +
      "ORDER BY objects.dtstart ASC LIMIT 1;\" 2>/dev/null | tr -d '\\n'"
    ]
    stdout: StdioCollector {
      onStreamFinished: function(text) {
        const raw = (text || "").trim()
        if (!raw) {
          root.displayText = ""
          root.tooltipText = "Omacal — no upcoming events"
          return
        }
        const parts = raw.split("|")
        const title = (parts[0] || "").trim() || "(no title)"
        const dt = (parts[1] || "").trim()
        const loc = (parts[2] || "").trim()
        let when = ""
        if (dt) {
          // dtstart is UTC RFC3339 or YYYY-MM-DD; show local HH:MM
          try {
            const d = new Date(dt)
            if (!isNaN(d.getTime())) {
              when = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
            } else {
              when = dt.slice(0, 16)
            }
          } catch (e) { when = dt.slice(0, 16) }
        }
        const shortTitle = title.length > 28 ? title.slice(0, 28) + "…" : title
        root.displayText = (when ? when + " " : "") + "󰃭 " + shortTitle
        root.tooltipText = (when ? when + " — " : "") + title + (loc ? " @ " + loc : "")
      }
    }
    onExited: function(exitCode) {
      if (exitCode !== 0) {
        // Keep last text on transient DB lock; clear only if empty
      }
    }
  }

  Timer {
    interval: 60000
    running: true
    repeat: true
    triggeredOnStart: true
    onTriggered: root.refresh()
  }

  // Fallback: also refresh when theme changes (DB may have been synced)
  Connections {
    target: root.bar
    function onBarForegroundChanged() { /* theme change may coincide with sync */ }
  }

  Item {
    anchors.fill: parent
    anchors.leftMargin: Style.space(6)
    anchors.rightMargin: Style.space(6)

    Text {
      id: label
      anchors.verticalCenter: parent.verticalCenter
      anchors.left: parent.left
      anchors.right: parent.right
      text: root.displayText
      color: root.bar ? root.bar.barForeground : Color.foreground
      font.family: root.bar ? root.bar.fontFamily : Style.font.family
      font.pixelSize: Style.font.body
      elide: Text.ElideRight
      opacity: 0.92
    }
  }

  MouseArea {
    anchors.fill: parent
    acceptedButtons: Qt.LeftButton | Qt.MiddleButton
    cursorShape: Qt.PointingHandCursor
    hoverEnabled: true
    onClicked: function(mouse) {
      if (root.bar) root.bar.run("omarchy-omacal")
      else Quickshell.execDetached(["omarchy-omacal"])
    }
    onEntered: if (root.bar) root.bar.showTooltip(root, root.tooltipText)
    onExited: if (root.bar) root.bar.hideTooltip(root)
  }
}
