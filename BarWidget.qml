import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// Bar icon that focuses the configurator window, or starts the app installed by install.sh.
BarWidget {
    id: root
    moduleName: "io.github.tg5ee.inglorious-core-linux"

    // nf-md-keyboard (U+F030C), drawn in the bar's Nerd Font so it follows the theme.
    readonly property string glyph: String.fromCodePoint(0xF030C)
    // Hyprland's dispatch parses Lua here, so focus goes through hl.dsp.focus like Omarchy's own helpers.
    readonly property string openCmd: "app=\"$HOME/.local/bin/inglorious-core\"; "
        + "if hyprctl clients -j | jq -e 'any(.[]; .class == \"inglorious-core\")' >/dev/null; then "
        + "hyprctl eval 'hl.dispatch(hl.dsp.focus({ window = \"class:^(inglorious-core)$\" }))'; "
        + "elif [ -x \"$app\" ]; then setsid -f \"$app\" >/dev/null 2>&1; "
        + "else notify-send 'Inglorious Core Linux' 'Not installed yet: run install.sh in the plugin folder.'; fi"

    implicitWidth: btn.implicitWidth
    implicitHeight: btn.implicitHeight
    visible: !vertical

    Process {
        id: openProcess
        command: ["bash", "-c", root.openCmd]
        running: false
    }

    BarIconButton {
        id: btn
        anchors.fill: parent
        bar: root.bar
        text: root.glyph
        tooltipText: "Inglorious Core Linux"
        onPressed: function(buttonCode) {
            if (buttonCode === Qt.LeftButton && !openProcess.running) openProcess.running = true
        }
    }
}
