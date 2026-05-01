# DMX Control

Software de control DMX (Tauri 2 + React + Rust).

Ver [PLAN.md](PLAN.md) para hoja de ruta y principios.

## Desarrollo

```sh
npm install
npm run tauri dev          # app de escritorio con hot reload
npm run lint               # biome
npm run build              # build del frontend
cd src-tauri && cargo test # tests del engine + drivers
```

## Logs

`~/Library/Logs/dmx-control/dmx-control.log` (rotación diaria).
Setear `RUST_LOG=trace` para subir verbosidad.

## Acceso a interfaces USB en macOS (Open DMX / ElectroTAS)

Las interfaces tipo Open DMX (ElectroTAS TZ-MINI, Enttec Open DMX USB, dongles
genéricos FTDI) se controlan desde libusb. En macOS, el kext del sistema
`AppleUSBFTDI` toma el chip al enchufarlo y bloquea el acceso directo. Tres
formas de destrabarlo:

1. **Más rápido (dev)**: `sudo npm run tauri dev`. El proceso con root puede
   pedirle al kernel que suelte el chip vía `detach_kernel_driver`.
2. **Más limpio (uso normal)**: instalar el [driver VCP firmado de FTDI](https://ftdichip.com/drivers/vcp-drivers/).
   Reemplaza a `AppleUSBFTDI` y coopera con libusb sin pedir root.
3. **Manual**: `sudo kextunload -bundle com.apple.driver.AppleUSBFTDI` (puede
   requerir SIP relajado según versión de macOS).

## Bindings TypeScript

Los tipos compartidos con Rust se generan con `ts-rs`. Para regenerarlos:

```sh
cd src-tauri && cargo test --lib export_bindings_
```

`npm run check-types` falla si quedaron desincronizados.
