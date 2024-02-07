# pgen_client - PGenerator client

Utility to control a Raspberry Pi with a PGenerator installation.  
Includes both manual & automatic calibration features.

Built on top of [egui](https://github.com/emilk/egui), [kolor](https://github.com/BoxDragon/kolor) and [ArgyllCMS](https://www.argyllcms.com).

## Features
- Settings to configure the PGenerator similar to `DeviceControl`.
  - Allows configuring the output colour format, HDR mode as well as metadata.
- Ability to send test patterns manually to the device.
- Can be used as an external pattern generator for DisplayCAL, through the Davinci Resolve interface.
- An internal pattern generator is available for manual calibration and measuring patch lists.
  - The internal generator uses `ArgyllCMS`' `spotread` utility to measure colour patches.

### Building
Dependencies:
- Linux: see [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) dependencies.

```bash
cargo build --release
```

&nbsp;

## Device
The Raspberry Pi must be setup with an installation of [PGenerator](https://www.avsforum.com/threads/dedicated-raspberry-pi-pgenerator-thread-set-up-configuration-updates-special-features-general-usage-tips.3167475).  
`pgen_client` was only tested with a Raspberry Pi 4B device. Some features may not be working on older devices.  

### Configuring the PGenerator output

First, the program communicates to the `PGenerator` device through TCP over the network.  
So you will need to start by figuring out the IP address to connect to.

You should then be able to connect to the default port, `85`.

Configurations:
- `Display mode`: the resolution/refresh rate combination to use for the display.
- `Color format`: RGB or YCbCr is possible, whether it works is dependent on the display.
- `Quant range`: Full/Limited range for the display output. Also requires the display to support the selected option.
- `Bit depth`: Sets the output bit depth for the HDMI data.
- `Colorimetry`: Sets the HDMI colorimetry flag. This is used by the display to interpret the pixels correctly.
- `Dynamic range`: Allows switching the output mode to `SDR`, `HDR10` or `Dolby Vision`.
  - This is handled by the `PGenerator` software. The display must support the selected mode.

`HDR metadata / DRM infoframe` is for the metadata signaling in HDMI for HDR output. It is only used for the `HDR(10)` mode.  
The `HDR10` mode may also switch to `HLG` if the `HLG` EOTF is selected.

With the exception of `Display mode` and `Dynamic range` configurations:
- All configuration changes require that the `PGenerator` software be restarted before they are applied to the output.  
  This can be done with either the `Restart PGenerator software` or the `Set AVI/DRM infoframe` buttons.

<a href="https://raw.githubusercontent.com/quietvoid/pgen_client/main/assets/01external-gen.jpg">
  <img src="https://raw.githubusercontent.com/quietvoid/pgen_client/main/assets/01external-gen.jpg" width="250">
</a>

### Configuring the test patterns

Once the `PGenerator` device is properly connected, test patterns can be displayed.
The most important settings here are:
- `Patch precision`: Whether to use 8 bit or 10 bit patches. This is independent of the output `Bit depth` configuration.
- `Patch size`: Size the patches take on the display, in % windows.
- `Position`: How the patches are positioned on the display. This can be a preset position or specific pixel coordinates.

With both internal/external pattern generators, the patches are sent at the configured size/position in `pgen_client`.

If the output is configured to the `Limited` Quant range, the `Limited range` checkbox must be checked.  
That will send patterns in limited RGB range to the `PGenerator`, ensuring correct patches are displayed.

For patch and background colours, they are either selected manually or through a pattern generator as described below.  
Patterns can be sent manually to test the configuration.

&nbsp;

## External pattern generator

Currently only supports displaying 10 bit patterns from DisplayCAL.  
DisplayCAL must be configured with a `Resolve` display.

**Instructions**:
1. To connect, start a calibration.
2. Select the `External` pattern generator  and click the `Start generator client` button.
3. DisplayCAL should start sending test patterns to the display.

&nbsp;

## Internal pattern generator
`pgen_client` can be used for simple manual calibration.  
It supports basic presets as well as the ability to load custom CSV patch lists.  
Usage is targeted at more advanced users that know how to interpret the measurements data.

`ArgyllCMS` must be installed on the system and the executables present in `PATH`.

> [!WARNING]
> I cannot guarantee that the displayed measurement data is accurate or even correct.  
> My knowledge of colour math is limited and I have not done extensive verification.  
> Do consider double checking results with other calibration software such as `DisplayCAL` or `HCFR`.

**Instructions**:
1. Select the `Internal` pattern generator.
1. Set up the `spotread` CLI arguments and start `spotread`.
2. Set the min/max target brightness as well as target primaries for the calibration.
3. Load a patch list to measure.
4. Measure all patches or select a single one and measure it.

<a href="https://raw.githubusercontent.com/quietvoid/pgen_client/main/assets/02internal-gen.jpg">
  <img src="https://raw.githubusercontent.com/quietvoid/pgen_client/main/assets/02internal-gen.jpg" width="250">
</a>
