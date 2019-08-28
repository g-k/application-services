/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.accounts

import mozilla.appservices.fxaclient.Device

typealias ServerConfig = mozilla.appservices.fxaclient.Config

/**
 * Configuration for the current device.
 *
 * @property name An initial name to use for the device record which will be created during authentication.
 * This can be changed later via [FxaDeviceConstellation].
 *
 * @property type Type of a device - mobile, desktop - used for displaying identifying icons on other devices.
 * This cannot be changed once device record is created.
 *
 * @property capabilities A set of device capabilities, such as SEND_TAB. This set can be expanded by
 * re-initializing [FxaAccountManager] with a new set (e.g. on app restart).
 * Shrinking a set of capabilities is currently not supported.
 */
data class DeviceConfig(
    val name: String,
    val type: Device.Type,
    val capabilities: Set<Device.Capability>
)