/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde_derive::*;

/// A client record.
#[derive(Clone, Debug, Eq, Deserialize, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    #[serde(rename = "id")]
    pub id: String,

    pub name: String,

    #[serde(default, rename = "type")]
    pub typ: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<ClientCommand>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fxa_device_id: Option<String>,

    /// `version`, `protocols`, `formfactor`, `os`, `appPackage`, `application`,
    /// and `device` are unused and optional in all implementations (Desktop,
    /// iOS, and Fennec), but we round-trip them.

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocols: Vec<String>,

    #[serde(default, rename = "formfactor" skip_serializing_if = "Option::is_none")]
    pub form_factor: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_package: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application: Option<String>,

    /// The model of the device, like "iPhone" or "iPod touch" on iOS. Note
    /// that this is _not_ the client ID (`id`) or the FxA device ID
    /// (`fxa_device_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

#[derive(Clone, Debug, Eq, Deserialize, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCommand {
    /// The command name. This is a string, not an enum, because we want to
    /// round-trip commands that we don't support yet.
    #[serde(rename = "command")]
    pub name: String,

    /// Extra, command-specific arguments. Note that we must send an empty
    /// array if the command expects no arguments.
    #[serde(default)]
    pub args: Vec<String>,

    /// Some commands, like repair, send a "flow ID" that other cliennts can
    /// record in their telemetry. We don't currently send commands with
    /// flow IDs, but we round-trip them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_id: Option<String>,
}
