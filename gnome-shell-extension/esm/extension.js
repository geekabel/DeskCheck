// DeskCheck pointer, for GNOME Shell 45 and later (ES modules).
//
// On Wayland an app cannot see where the mouse is outside its own windows.
// GNOME Shell can: this extension answers one D-Bus call on the session
// bus, GetPosition, with the place of the mouse on the desktop.
// It reads nothing else and changes nothing in GNOME Shell.

import Gio from 'gi://Gio';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS_NAME = 'io.github.geekabel.DeskCheck.Pointer';
const OBJECT_PATH = '/io/github/geekabel/DeskCheck/Pointer';
const INTERFACE = `
<node>
  <interface name="io.github.geekabel.DeskCheck.Pointer">
    <method name="GetPosition">
      <arg type="i" direction="out" name="x"/>
      <arg type="i" direction="out" name="y"/>
    </method>
  </interface>
</node>`;

export default class DeskCheckPointer extends Extension {
    enable() {
        this._exported = Gio.DBusExportedObject.wrapJSObject(INTERFACE, this);
        this._exported.export(Gio.DBus.session, OBJECT_PATH);
        this._ownerId = Gio.bus_own_name(Gio.BusType.SESSION, BUS_NAME,
            Gio.BusNameOwnerFlags.NONE, null, null, null);
    }

    disable() {
        Gio.bus_unown_name(this._ownerId);
        this._exported.unexport();
        this._exported = null;
    }

    // Two out arguments: GJS sends the array as (ii).
    GetPosition() {
        const [x, y] = global.get_pointer();
        return [x, y];
    }
}
