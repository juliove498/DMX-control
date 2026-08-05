// Single source of truth for every user-facing string in the desktop UI.
// Keep the EN and ES objects shape-identical: TypeScript pins both via
// `Translation` so a forgotten key in either locale fails the build
// instead of silently rendering an English string in a Spanish UI (or
// vice-versa). Add new keys to BOTH locales in the same commit.
//
// Migration policy: components opt in by replacing literal strings with
// `t("key")` one screen at a time. Strings not yet migrated stay
// hardcoded — that's how we ship i18n without a 100-file PR.

export type Translation = {
  // ---- App shell --------------------------------------------------------
  "app.brand": string;
  "app.tab.stage": string;
  "app.tab.scenes": string;
  "app.tab.loops": string;
  "app.tab.chaser": string;
  "app.tab.movement": string;
  "app.tab.preview3d": string;
  "app.tab.config": string;
  "app.openInOtherWindow": string;
  "app.fullscreen": string;
  "app.fullscreenHint": string;
  "app.button.new": string;
  "app.button.open": string;
  "app.button.save": string;
  "app.global.blackout": string;
  "app.global.blackoutTitle": string;
  "app.global.blind": string;
  "app.global.blindTitle": string;
  "app.global.bpm.label": string;
  "app.global.bpm.tap": string;
  "app.global.bpm.tapHint": string;
  "app.global.bpm.on": string;
  "app.global.bpm.off": string;
  "app.global.bpm.toggleHint": string;
  "app.global.bpm.editHint": string;
  "app.global.bpm.recordPattern": string;
  "app.global.bpm.recordPatternHint": string;
  "app.global.bpm.recordingPattern": string;
  "app.global.bpm.recordingHint": string;
  "app.global.bpm.clearPattern": string;
  "app.global.bpm.clearPatternHint": string;
  "app.global.bpm.patternActive": string;
  "app.show.untitled": string;
  "app.show.unsaved": string;
  "app.show.renameHint": string;
  "app.show.renameHintEmpty": string;
  "app.dialog.closeTitle": string;
  "app.dialog.closeBody": string;
  "app.toast.savedAt": string;
  "app.toast.saveError": string;
  "app.toast.renameError": string;

  // ---- Config tabs ------------------------------------------------------
  "config.tabs.library": string;
  "config.tabs.libraryHint": string;
  "config.tabs.outputs": string;
  "config.tabs.outputsHint": string;
  "config.tabs.patch": string;
  "config.tabs.patchHint": string;
  "config.tabs.blackoutBlind": string;
  "config.tabs.blackoutBlindHint": string;
  "config.tabs.bindings": string;
  "config.tabs.bindingsHint": string;
  "config.tabs.midi": string;
  "config.tabs.midiHint": string;
  "config.tabs.streamdeck": string;
  "config.tabs.streamdeckHint": string;
  "config.tabs.ai": string;
  "config.tabs.aiHint": string;
  "config.tabs.remote": string;
  "config.tabs.remoteHint": string;
  "config.tabs.sync": string;
  "config.tabs.syncHint": string;
  "config.tabs.vdj": string;
  "config.tabs.vdjHint": string;
  "config.tabs.general": string;
  "config.tabs.generalHint": string;
  "config.tabs.direct": string;
  "config.tabs.directHint": string;

  // ---- VirtualDJ poller -------------------------------------------------
  "vdj.title": string;
  "vdj.intro": string;
  "vdj.section.connection": string;
  "vdj.section.behavior": string;
  "vdj.host": string;
  "vdj.hostHint": string;
  "vdj.port": string;
  "vdj.portHint": string;
  "vdj.bearer": string;
  "vdj.bearerHint": string;
  "vdj.interval": string;
  "vdj.intervalHint": string;
  "vdj.enabled": string;
  "vdj.enabledHint": string;
  "vdj.save": string;
  "vdj.statusSection": string;
  "vdj.status.running": string;
  "vdj.status.stopped": string;
  "vdj.status.lastBpm": string;
  "vdj.status.lastUpdate": string;
  "vdj.status.never": string;
  "vdj.status.error": string;
  "vdj.status.secondsAgo": string;
  "vdj.errSave": string;
  "vdj.halve.enabled": string;
  "vdj.halve.enabledHint": string;
  "vdj.halve.threshold": string;
  "vdj.halve.thresholdHint": string;

  // ---- Loops view -------------------------------------------------------
  "loops.title": string;
  "loops.meta": string;
  "loops.intro": string;
  "loops.new": string;
  "loops.empty": string;
  "loops.emptyScenes": string;
  "loops.sceneCount": string;
  "loops.startHint": string;
  "loops.stopHint": string;
  "loops.deleteTitle": string;
  "loops.deleteBody": string;
  "loops.hold": string;
  "loops.holdHint": string;
  "loops.missingScene": string;
  "loops.moveUp": string;
  "loops.moveDown": string;
  "loops.removeFromList": string;
  "loops.addPlaceholder": string;
  "loops.repeatOption": string;
  "loops.addSceneGroup": string;
  "loops.addSnapshotGroup": string;
  "loops.snapshotChipHint": string;
  "loops.holdDisabledHint": string;
  "loops.syncBpm": string;
  "loops.syncBpmHint": string;
  "loops.syncBpmOff": string;
  "loops.subdivisionHint": string;
  "loops.sub.quarter": string;
  "loops.sub.half": string;
  "loops.sub.one": string;
  "loops.sub.two": string;
  "loops.sub.four": string;
  "loops.sub.eight": string;
  "scenes.list.activeLoopPrefix": string;
  "scenes.list.activeLoopStop": string;

  // ---- General (language picker) ----------------------------------------
  "general.title": string;
  "general.language": string;
  "general.languageHint": string;
  "general.language.en": string;
  "general.language.es": string;
  "general.showFile": string;
  "general.showFileHint": string;

  // ---- Sync view --------------------------------------------------------
  "sync.title": string;
  "sync.intro": string;
  "sync.section.token": string;
  "sync.token.saved": string;
  "sync.token.delete": string;
  "sync.token.deleteConfirm": string;
  "sync.token.placeholder": string;
  "sync.token.save": string;
  "sync.token.saving": string;
  "sync.token.connectedAs": string;
  "sync.token.help": string;
  "sync.section.settings": string;
  "sync.machineLabel": string;
  "sync.machineLabel.placeholder": string;
  "sync.gistId": string;
  "sync.gistId.placeholder": string;
  "sync.includeOutputs": string;
  "sync.saveSettings": string;
  "sync.section.status": string;
  "sync.status.lastPush": string;
  "sync.status.lastPull": string;
  "sync.status.lastRemote": string;
  "sync.status.never": string;
  "sync.status.from": string;
  "sync.probe": string;
  "sync.probing": string;
  "sync.probe.empty": string;
  "sync.probe.ahead": string;
  "sync.probe.upToDate": string;
  "sync.section.actions": string;
  "sync.action.push": string;
  "sync.action.pushing": string;
  "sync.action.forcePush": string;
  "sync.action.forcePushTitle": string;
  "sync.action.pull": string;
  "sync.action.pulling": string;
  "sync.action.pullConfirm": string;
  "sync.backupHint": string;
  "sync.toast.pushOk": string;
  "sync.toast.pullOk": string;

  // ---- Remote bridge view ----------------------------------------------
  "remote.title": string;
  "remote.intro": string;
  "remote.status": string;
  "remote.statusRunning": string;
  "remote.statusStopped": string;
  "remote.start": string;
  "remote.stop": string;
  "remote.connectManually": string;
  "remote.clientsConnected": string;
  "remote.pairing": string;
  "remote.pairing.expiresIn": string;
  "remote.pairing.cancel": string;
  "remote.pairing.start": string;
  "remote.devices": string;
  "remote.devices.empty": string;
  "remote.devices.pairedAgo": string;
  "remote.devices.seenAgo": string;
  "remote.devices.revoke": string;
  "remote.devices.revokeConfirm": string;
  "remote.time.secAgo": string;
  "remote.time.minAgo": string;
  "remote.time.hAgo": string;
  "remote.time.dAgo": string;

  // ---- Common buttons ---------------------------------------------------
  "common.cancel": string;
  "common.save": string;
  "common.delete": string;
  "common.loading": string;
  "common.empty": string;
  "common.add": string;
  "common.remove": string;
  "common.edit": string;
  "common.close": string;
  "common.refresh": string;
  "common.connect": string;
  "common.disconnect": string;
  "common.connected": string;
  "common.disconnected": string;
  "common.universe": string;
  "common.address": string;
  "common.channel": string;
  "common.fixture": string;
  "common.fixtures": string;
  "common.mode": string;
  "common.name": string;
  "common.value": string;
  "common.enabled": string;
  "common.disabled": string;
  "common.optional": string;
  "common.yes": string;
  "common.no": string;

  // ---- Direct output (debug faders) ------------------------------------
  "direct.title": string;
  "direct.fpsNone": string;
  "direct.fpsValue": string;
  "direct.statsMeta": string;
  "direct.master": string;
  "direct.blackoutOn": string;
  "direct.blackout": string;
  "direct.clearAll": string;

  // ---- Library (fixture defs) ------------------------------------------
  "library.title": string;
  "library.search": string;
  "library.reload": string;
  "library.uploadImage": string;
  "library.changeImage": string;
  "library.errorOpenDialog": string;
  "library.errorSetImage": string;
  "library.modeSummary": string;
  "library.filesPath": string;
  "library.gobosToggle": string;
  "library.gobosCollapse": string;
  "library.gobosEmpty": string;
  "library.gobosModeFmt": string;
  "library.gobosChannelFmt": string;
  "library.gobosUpload": string;
  "library.gobosReplace": string;
  "library.gobosRangeFmt": string;
  "library.gobosErrorOversize": string;

  // ---- Outputs ---------------------------------------------------------
  "outputs.title": string;
  "outputs.kind.mock": string;
  "outputs.kind.artNet": string;
  "outputs.kind.sacn": string;
  "outputs.kind.enttec": string;
  "outputs.kind.openDmx": string;
  "outputs.kind.openDmxFtdi": string;
  "outputs.add.mock": string;
  "outputs.add.artNet": string;
  "outputs.artnetScan.button": string;
  "outputs.artnetScan.hint": string;
  "outputs.artnetScan.scanning": string;
  "outputs.artnetScan.title": string;
  "outputs.artnetScan.rescan": string;
  "outputs.artnetScan.close": string;
  "outputs.artnetScan.inProgress": string;
  "outputs.artnetScan.none": string;
  "outputs.artnetScan.universesOut": string;
  "outputs.artnetScan.add": string;
  "outputs.artnetScan.addHint": string;
  "outputs.add.sacn": string;
  "outputs.add.enttec": string;
  "outputs.add.openDmx": string;
  "outputs.add.openDmxFtdi": string;
  "outputs.rescan": string;
  "outputs.field.target": string;
  "outputs.field.sourceName": string;
  "outputs.field.priority": string;
  "outputs.field.serialPort": string;
  "outputs.field.ftdiDevice": string;
  "outputs.field.dtrHigh": string;
  "outputs.field.rtsHigh": string;
  "outputs.field.universes": string;
  "outputs.placeholder.select": string;
  "outputs.tag.ftdi": string;
  "outputs.tag.offline": string;
  "outputs.tag.inUse": string;
  "outputs.zadigHint": string;
  "outputs.remove": string;
  "outputs.empty": string;
  "outputs.dirty": string;
  "outputs.ok": string;
  "outputs.discard": string;
  "outputs.apply": string;

  // ---- Patch -----------------------------------------------------------
  "patch.title": string;
  "patch.statusClean": string;
  "patch.statusIssues": string;
  "patch.qty": string;
  "patch.universe": string;
  "patch.address": string;
  "patch.labelPlaceholder": string;
  "patch.add": string;
  "patch.modeFmt": string;
  "patch.col.id": string;
  "patch.col.label": string;
  "patch.col.fixture": string;
  "patch.col.mode": string;
  "patch.col.universe": string;
  "patch.col.address": string;
  "patch.col.range": string;
  "patch.del": string;
  "patch.unknownDef": string;
  "patch.conflictFmt": string;

  // ---- Buttons (blackout & blind) --------------------------------------
  "buttons.title": string;
  "buttons.subhead": string;
  "buttons.section.master": string;
  "buttons.section.blackout": string;
  "buttons.section.blind": string;
  "buttons.masterIntro": string;
  "buttons.assignedMaster": string;
  "buttons.assignedMasterAuto": string;
  "buttons.emptyMaster": string;
  "buttons.masterHintAuto": string;
  "buttons.masterHintCustom": string;
  "buttons.blackoutIntro": string;
  "buttons.blindIntro": string;
  "buttons.fadeIn": string;
  "buttons.fadeOut": string;
  "buttons.assignedBlackout": string;
  "buttons.assignedBlackoutAuto": string;
  "buttons.assignedBlind": string;
  "buttons.emptyBlackout": string;
  "buttons.emptyBlind": string;
  "buttons.blackoutHintAuto": string;
  "buttons.blackoutHintCustom": string;
  "buttons.blindHintAuto": string;
  "buttons.blindHintCustom": string;

  // ---- MIDI ------------------------------------------------------------
  "midi.title": string;
  "midi.intro": string;
  "midi.devices": string;
  "midi.refresh": string;
  "midi.connect": string;
  "midi.disconnect": string;
  "midi.connected": string;
  "midi.notConnected": string;
  "midi.lastEvent": string;
  "midi.colorTest": string;
  "midi.empty": string;
  "midi.refreshHint": string;
  "midi.detectedDevices": string;
  "midi.errListing": string;
  "midi.errConnect": string;
  "midi.errDisconnect": string;
  "midi.errTest": string;
  "midi.in": string;
  "midi.out": string;
  "midi.bothIO": string;
  "midi.onlyIn": string;
  "midi.surfaceActive": string;
  "midi.testPads": string;
  "midi.statusSection": string;
  "midi.connectedToFmt": string;

  // ---- Stream Deck -----------------------------------------------------
  "streamdeck.title": string;
  "streamdeck.intro": string;
  "streamdeck.refresh": string;
  "streamdeck.connect": string;
  "streamdeck.disconnect": string;
  "streamdeck.empty": string;
  "streamdeck.detectedDevices": string;
  "streamdeck.deviceMeta": string;
  "streamdeck.statusSection": string;
  "streamdeck.connectedToFmt": string;
  "streamdeck.errListing": string;
  "streamdeck.errConnect": string;
  "streamdeck.errDisconnect": string;

  // ---- AI Config -------------------------------------------------------
  "ai.title": string;
  "ai.intro": string;
  "ai.provider": string;
  "ai.provider.anthropic": string;
  "ai.provider.openai": string;
  "ai.apiKey": string;
  "ai.apiKey.show": string;
  "ai.apiKey.hide": string;
  "ai.model": string;
  "ai.test": string;
  "ai.testing": string;
  "ai.testOk": string;
  "ai.testFail": string;
  "ai.save": string;
  "ai.saved": string;
  "ai.refreshModels": string;
  "ai.subhead": string;
  "ai.warning": string;
  "ai.providerActive": string;
  "ai.provider.none": string;
  "ai.provider.anthropicLong": string;
  "ai.provider.openaiLong": string;
  "ai.section.anthropic": string;
  "ai.section.openai": string;
  "ai.keyHint.anthropic": string;
  "ai.keyHint.openai": string;
  "ai.toggleKey.show": string;
  "ai.toggleKey.hide": string;
  "ai.testDisabledHint": string;
  "ai.testActiveHint": string;
  "ai.savedToast": string;
  "ai.errPrefix": string;

  // ---- Scenes view -----------------------------------------------------
  "scenes.title": string;
  "scenes.metaSummary": string;
  "scenes.aiTrigger": string;
  "scenes.aiTriggerHint": string;
  "scenes.errCreate": string;
  "scenes.list.new": string;
  "scenes.list.empty": string;
  "scenes.list.lpHint": string;
  "scenes.list.recallHint": string;
  "scenes.list.stepCount": string;
  "scenes.list.activePrefix": string;
  "scenes.list.activeStep": string;
  "scenes.list.release": string;
  "scenes.editor.empty": string;
  "scenes.editor.recallHint": string;
  "scenes.editor.go": string;
  "scenes.editor.cycleTotal": string;
  "scenes.editor.aiIterate": string;
  "scenes.editor.aiIterateHint": string;
  "scenes.editor.delete": string;
  "scenes.editor.deleteConfirm": string;
  "scenes.editor.fxHint": string;
  "scenes.editor.stepsHeading": string;
  "scenes.editor.stepsEmpty": string;
  "scenes.editor.addStepHeading": string;
  "scenes.editor.fadeIn": string;
  "scenes.editor.hold": string;
  "scenes.editor.touchedOnly": string;
  "scenes.editor.addStep": string;
  "scenes.editor.addDisabledHint": string;
  "scenes.editor.addEnabledHint": string;
  "scenes.editor.loopHint": string;
  "scenes.editor.fixturesHint": string;
  "scenes.editor.clearProg": string;
  "scenes.programmer.label": string;
  "scenes.programmer.clear": string;
  "snapshots.heading": string;
  "snapshots.capture": string;
  "snapshots.captureHint": string;
  "snapshots.empty": string;
  "snapshots.activateHint": string;
  "snapshots.deactivate": string;
  "snapshots.deactivateHint": string;
  "snapshots.recaptureHint": string;
  "snapshots.deleteConfirm": string;
  "snapshots.deleteHint": string;
  "snapshots.activeBadge": string;
  "scenes.step.fade": string;
  "scenes.step.hold": string;
  "scenes.step.ms": string;
  "scenes.step.metaFmt": string;
  "scenes.step.placeholder": string;
  "scenes.step.update": string;
  "scenes.step.updateAllHint": string;
  "scenes.step.updateTouched": string;
  "scenes.step.updateTouchedHint": string;
  "scenes.step.removeHint": string;
  "scenes.step.removeOnlyHint": string;
  "scenes.step.removeConfirm": string;
  "scenes.fx.chaser": string;
  "scenes.fx.movement": string;
  "scenes.fx.inherit": string;
  "scenes.fx.disable": string;
  "scenes.fx.enable": string;
  "scenes.fx.noOptions": string;

  // ---- Chaser view -----------------------------------------------------
  "chaser.title": string;
  "chaser.addExample": string;
  "chaser.addNew": string;
  "chaser.empty": string;
  "chaser.exampleNoticeNoFixtures": string;
  "chaser.exampleNoticeWithFixtures": string;
  "chaser.toggle.on": string;
  "chaser.toggle.off": string;
  "chaser.toggle.disable": string;
  "chaser.toggle.enable": string;
  "chaser.lpHint": string;
  "chaser.summary": string;
  "chaser.summary.fade": string;
  "chaser.hide": string;
  "chaser.edit": string;
  "chaser.del": string;
  "chaser.tab.pattern": string;
  "chaser.tab.timing": string;
  "chaser.tab.slots": string;
  "chaser.section.pattern": string;
  "chaser.section.colorMode": string;
  "chaser.section.tempo": string;
  "chaser.section.levels": string;
  "chaser.section.fade": string;
  "chaser.bpm": string;
  "chaser.subdivision": string;
  "chaser.master": string;
  "chaser.background": string;
  "chaser.fadeEnabled": string;
  "chaser.fadeAmount": string;
  "chaser.fadeCurve": string;
  "chaser.fadeHint": string;
  "chaser.preview.empty": string;
  "chaser.slots.empty": string;
  "chaser.slots.intLabel": string;
  "chaser.slots.colorLabel": string;
  "chaser.slots.add": string;
  "chaser.slots.fixtureFmt": string;
  "chaser.cadence": string;
  "chaser.cadenceN": string;
  "chaser.rotation": string;
  "chaser.removeColor": string;
  "chaser.addColor": string;
  "chaser.rainbowSpeed": string;
  "chaser.rainbowSpread": string;
  "chaser.fadeCurveLabel.linear": string;
  "chaser.fadeCurveLabel.easeInOut": string;
  "chaser.fadeCurveLabel.easeIn": string;
  "chaser.fadeCurveLabel.easeOut": string;
  "chaser.fadeCurveLabel.exponential": string;
  "chaser.fadeCurveLabel.logarithmic": string;
  "chaser.subdivisionLabel.16": string;
  "chaser.subdivisionLabel.8": string;
  "chaser.subdivisionLabel.4": string;
  "chaser.subdivisionLabel.2": string;
  "chaser.subdivisionLabel.1": string;
  "chaser.patternLabel.allTogether": string;
  "chaser.patternLabel.alternate": string;
  "chaser.patternLabel.chase": string;
  "chaser.patternLabel.chaseReverse": string;
  "chaser.patternLabel.pingPong": string;
  "chaser.patternLabel.wave": string;
  "chaser.patternLabel.waveReverse": string;
  "chaser.patternLabel.build": string;
  "chaser.patternLabel.buildReverse": string;
  "chaser.patternLabel.centerOut": string;
  "chaser.patternLabel.symmetric": string;
  "chaser.patternLabel.random": string;
  "chaser.patternLabel.outsideIn": string;
  "chaser.patternLabel.invertedChase": string;
  "chaser.patternLabel.groupsOfTwo": string;
  "chaser.patternLabel.groupsOfThree": string;
  "chaser.patternLabel.halfSwap": string;
  "chaser.patternLabel.edges": string;
  "chaser.patternLabel.pulseOut": string;
  "chaser.patternLabel.pulseIn": string;
  "chaser.patternLabel.accordion": string;
  "chaser.patternLabel.bowtie": string;
  "chaser.patternLabel.dualChase": string;
  "chaser.patternLabel.symmetricBounce": string;
  "chaser.cadenceLabel.everyStep": string;
  "chaser.cadenceLabel.everyN": string;
  "chaser.cadenceLabel.perSlot": string;
  "chaser.cadenceLabel.alternateSlots": string;
  "chaser.cadenceLabel.chasePerColor": string;
  "chaser.rotationLabel.perStep": string;
  "chaser.rotationLabel.perCycle": string;
  "chaser.rotationLabel.perSlot": string;
  "chaser.colorModeLabel.disabled": string;
  "chaser.colorModeLabel.single": string;
  "chaser.colorModeLabel.twoColor": string;
  "chaser.colorModeLabel.palette": string;
  "chaser.colorModeLabel.rainbow": string;

  // ---- Movement view ---------------------------------------------------
  "movement.title": string;
  "movement.addNew": string;
  "movement.empty": string;
  "movement.delete": string;
  "movement.deleteConfirm": string;
  "movement.lpHint": string;
  "movement.legend.fixtures": string;
  "movement.legend.shape": string;
  "movement.legend.canon": string;
  "movement.legend.timing": string;
  "movement.legend.transform": string;
  "movement.fixtures.empty": string;
  "movement.fixtures.invertPan": string;
  "movement.fixtures.invertTilt": string;
  "movement.fixtures.add": string;
  "movement.fixtures.fixtureFmt": string;
  "movement.shape.sides": string;
  "movement.shape.points": string;
  "movement.shape.innerRatio": string;
  "movement.timing.bpm": string;
  "movement.timing.loopLength": string;
  "movement.transform.sizeX": string;
  "movement.transform.sizeY": string;
  "movement.transform.centerX": string;
  "movement.transform.centerY": string;
  "movement.transform.rotation": string;
  "movement.transform.reset": string;
  "movement.previewHint": string;
  "movement.previewBadgeOff": string;
  "movement.previewMeta": string;
  "movement.previewPaused": string;
  "movement.wave.pan": string;
  "movement.wave.tilt": string;
  "movement.wave.waveform": string;
  "movement.wave.frequency": string;
  "movement.wave.phaseShift": string;
  "movement.wave.amplitude": string;
  "movement.wave.offset": string;
  "movement.preset.label": string;
  "movement.preset.loadHint": string;
  "movement.shapeLabel.circle": string;
  "movement.shapeLabel.polygon": string;
  "movement.shapeLabel.star": string;
  "movement.shapeLabel.figureEight": string;
  "movement.shapeLabel.lineH": string;
  "movement.shapeLabel.lineV": string;
  "movement.shapeLabel.sineCombo": string;
  "movement.spreadLabel.none": string;
  "movement.spreadLabel.even": string;
  "movement.spreadLabel.symmetric": string;
  "movement.spreadLabel.pairs": string;
  "movement.spreadLabel.manual": string;
  "movement.directionLabel.forward": string;
  "movement.directionLabel.reverse": string;
  "movement.directionLabel.pingPong": string;
  "movement.subdivLabel.16": string;
  "movement.subdivLabel.8": string;
  "movement.subdivLabel.1beat": string;
  "movement.subdivLabel.2beats": string;
  "movement.subdivLabel.4beats": string;
  "movement.subdivLabel.8beats": string;
  "movement.waveformLabel.sine": string;
  "movement.waveformLabel.cosine": string;
  "movement.waveformLabel.triangle": string;
  "movement.waveformLabel.square": string;
  "movement.waveformLabel.sawtooth": string;
  "movement.waveformLabel.rampUp": string;
  "movement.waveformLabel.rampDown": string;
  "movement.presetLabel.circle": string;
  "movement.presetLabel.figureEight": string;
  "movement.presetLabel.lissajous32": string;
  "movement.presetLabel.lissajous54": string;
  "movement.presetLabel.wavePan": string;

  // ---- Stage view ------------------------------------------------------
  "stage.title": string;
  "stage.meta": string;
  "stage.metaSelected": string;
  "stage.empty": string;
  "stage.sidebar.placeholder": string;
  "stage.fixture.untouchAria": string;
  "stage.fixture.untouchTitle": string;
  "stage.type.selectHint": string;
  "stage.editor.unavailable": string;
  "stage.editor.multiHeading": string;
  "stage.editor.multiBadge": string;
  "stage.editor.mixedHint": string;
  "stage.editor.activeFx": string;
  "stage.editor.fxKindChaser": string;
  "stage.editor.fxKindMove": string;
  "stage.editor.fxTooltipChaser": string;
  "stage.editor.fxTooltipMovement": string;
  "stage.editor.center": string;
  "stage.editor.home": string;
  "stage.editor.section.intStrobe": string;
  "stage.editor.section.color": string;
  "stage.editor.section.extras": string;
  "stage.fxbar.scenes": string;
  "stage.fxbar.snapshots": string;
  "stage.fxbar.movements": string;
  "stage.fxbar.chasers": string;
  "stage.fxbar.releaseHint": string;
  "stage.fxbar.idle": string;
  "stage.fxbar.openScenes": string;
  "stage.fxbar.closeScenes": string;
  "stage.fxbar.scenesBtnClose": string;
  "stage.fxbar.scenesBtnOpen": string;
  "stage.fxbar.morePill": string;
  "stage.fxbar.scenePillTitle": string;
  "stage.fxbar.disableMovement": string;
  "stage.fxbar.enableMovement": string;
  "stage.fxbar.disableChaser": string;
  "stage.fxbar.enableChaser": string;
  "stage.fxbar.stepName": string;
  "stage.fxbar.stepFmt": string;
  "stage.sqp.aria": string;
  "stage.sqp.title": string;
  "stage.sqp.metaFmt": string;
  "stage.sqp.list": string;
  "stage.sqp.newScene": string;
  "stage.sqp.empty": string;
  "stage.sqp.recallTitle": string;
  "stage.sqp.editSceneTitle": string;
  "stage.sqp.sceneStepFmt": string;
  "stage.sqp.sceneLive": string;
  "stage.sqp.releaseActive": string;
  "stage.sqp.stepsHeading": string;
  "stage.sqp.tagLive": string;
  "stage.sqp.tagStopped": string;
  "stage.sqp.followActive": string;
  "stage.sqp.followActiveHint": string;
  "stage.sqp.stepPlaceholder": string;
  "stage.sqp.fadeTitle": string;
  "stage.sqp.holdTitle": string;
  "stage.sqp.overwriteAll": string;
  "stage.sqp.overwriteTouched": string;
  "stage.sqp.removeStep": string;
  "stage.sqp.removeOnlyHint": string;
  "stage.sqp.recordHeadingNew": string;
  "stage.sqp.recordHeadingAdd": string;
  "stage.sqp.fade": string;
  "stage.sqp.hold": string;
  "stage.sqp.ms": string;
  "stage.sqp.touchedOnly": string;
  "stage.sqp.addStep": string;
  "stage.sqp.recordNew": string;
  "stage.sqp.touchedHelperOn": string;
  "stage.sqp.touchedHelperOff": string;
  "stage.sqp.touchedToggle": string;
  "stage.sqp.locateHint": string;
  "stage.sqp.locate": string;
  "stage.sqp.clearProg": string;
  "stage.sqp.errRecord": string;
  "stage.sqp.errAddStep": string;
  "stage.sqp.errCreate": string;
  "stage.sqp.errNoTouched": string;
  "stage.menu.fixtureSingular": string;
  "stage.menu.fixturePlural": string;
  "stage.menu.centerPanTilt": string;
  "stage.menu.park": string;
  "stage.menu.fullIntensity": string;
  "stage.menu.blackoutFixture": string;
  "stage.menu.rename": string;
  "stage.menu.duplicate": string;
  "stage.menu.untouch": string;
  "stage.menu.remove": string;
  "stage.confirm.removeOne": string;
  "stage.confirm.removeMany": string;

  // ---- AI Generate Modal -----------------------------------------------
  "aiGen.aria": string;
  "aiGen.title": string;
  "aiGen.titleIterating": string;
  "aiGen.errPromptEmpty": string;
  "aiGen.errScopeEmpty": string;
  "aiGen.errRefineEmpty": string;
  "aiGen.field.prompt": string;
  "aiGen.field.promptPlaceholder": string;
  "aiGen.field.stepCount": string;
  "aiGen.field.scope": string;
  "aiGen.scope.all": string;
  "aiGen.scope.selected": string;
  "aiGen.scope.fixturesHint": string;
  "aiGen.disabledHint": string;
  "aiGen.activeHint": string;
  "aiGen.generating": string;
  "aiGen.generate": string;
  "aiGen.preview.stepCount": string;
  "aiGen.preview.stepPlaceholder": string;
  "aiGen.preview.stepMeta": string;
  "aiGen.refine": string;
  "aiGen.refinePlaceholder": string;
  "aiGen.refineHint": string;
  "aiGen.refining": string;
  "aiGen.refineLabel": string;
  "aiGen.resetFromScratch": string;
  "aiGen.replaceOriginal": string;
  "aiGen.replaceOriginalHint": string;
  "aiGen.replacing": string;
  "aiGen.applyNew": string;
  "aiGen.applying": string;

  // ---- Preview 3D fixtures panel ---------------------------------------
  "p3d.types.title": string;
  "p3d.types.intro": string;
  "p3d.types.metaPrefix": string;
  "p3d.types.brightness": string;
  "p3d.types.brightnessHint": string;
  "p3d.types.beamOverride": string;
  "p3d.types.beamMin": string;
  "p3d.types.beamMax": string;
  "p3d.types.beamMinHint": string;
  "p3d.types.beamMaxHint": string;
  "p3d.types.prismOverride": string;
  "p3d.types.prismThreshold": string;
  "p3d.types.prismFacets": string;
  "p3d.types.prismSplay": string;
  "p3d.types.prismThresholdHint": string;
  "p3d.types.prismFacetsHint": string;
  "p3d.types.prismSplayHint": string;
  "p3d.types.reset": string;
};

export const en: Translation = {
  "app.brand": "DMX Control",
  "app.tab.stage": "Stage",
  "app.tab.scenes": "Scenes",
  "app.tab.loops": "Loops",
  "app.tab.chaser": "Chaser",
  "app.tab.movement": "Movement",
  "app.tab.preview3d": "Preview 3D",
  "app.tab.config": "Config",
  "app.openInOtherWindow": "Open {name} in another window",
  "app.fullscreen": "Fullscreen",
  "app.fullscreenHint": "Fullscreen (F11)",
  "app.button.new": "New",
  "app.button.open": "Open…",
  "app.button.save": "Save",
  "app.global.blackout": "BLACKOUT",
  "app.global.blackoutTitle": "Blackout (toggle, configurable fades)",
  "app.global.blind": "BLIND",
  "app.global.blindTitle": "Blind / blinder (hold, halogen with fade in/out)",
  "app.global.bpm.label": "BPM",
  "app.global.bpm.tap": "TAP",
  "app.global.bpm.tapHint":
    "Tap repeatedly to set the overall BPM from your timing. After two taps the value updates and the override turns on.",
  "app.global.bpm.on": "ON",
  "app.global.bpm.off": "OFF",
  "app.global.bpm.toggleHint":
    "When ON, every chaser and movement runs at this BPM instead of its own configured tempo.",
  "app.global.bpm.editHint": "Type a BPM (20–300) and press Enter to set it.",
  "app.global.bpm.recordPattern": "REC",
  "app.global.bpm.recordPatternHint":
    "Record a syncopated rhythm (clave, cha-cha…). Lock the BPM first, then click to start recording and tap the pattern. Click again to commit.",
  "app.global.bpm.recordingPattern": "STOP",
  "app.global.bpm.recordingHint": "Recording… tap the pattern, then click to commit.",
  "app.global.bpm.clearPattern": "✕",
  "app.global.bpm.clearPatternHint": "Clear the active pattern; chasers go back to plain BPM.",
  "app.global.bpm.patternActive": "{count} hits / {bars} bar(s)",
  "app.show.untitled": "Untitled",
  "app.show.unsaved": "(unsaved) · click to rename",
  "app.show.renameHint": "{path} · click to rename",
  "app.show.renameHintEmpty": "(unsaved) · click to rename",
  "app.dialog.closeTitle": "Close app",
  "app.dialog.closeBody": "Close DMX Control?\nAll open popout windows will close too.",
  "app.toast.savedAt": "Saved to {path}",
  "app.toast.saveError": "Save failed: {err}",
  "app.toast.renameError": "Could not rename: {err}",

  "config.tabs.library": "Library",
  "config.tabs.libraryHint": "Fixture definitions",
  "config.tabs.outputs": "Outputs",
  "config.tabs.outputsHint": "DMX universes and drivers",
  "config.tabs.patch": "Patch",
  "config.tabs.patchHint": "Assign fixtures to DMX addresses",
  "config.tabs.blackoutBlind": "Globals",
  "config.tabs.blackoutBlindHint":
    "Master scaling, blackout fades, blind halogen — per-fixture channel sets",
  "config.tabs.bindings": "Buttons",
  "config.tabs.bindingsHint": "Custom Launchpad + Stream Deck button assignments",
  "config.tabs.midi": "MIDI",
  "config.tabs.midiHint": "Control surface (Launchpad, etc.)",
  "config.tabs.streamdeck": "Stream Deck",
  "config.tabs.streamdeckHint": "Elgato Stream Deck control surface",
  "config.tabs.ai": "AI",
  "config.tabs.aiHint": "Provider, model and API key for LLM scene generation",
  "config.tabs.remote": "Remote",
  "config.tabs.remoteHint": "LAN bridge for the iPhone companion app",
  "config.tabs.sync": "Sync",
  "config.tabs.syncHint": "Share config across machines via private GitHub Gist",
  "config.tabs.vdj": "VirtualDJ",
  "config.tabs.vdjHint": "Pull tempo from VirtualDJ's Network Control Plugin",
  "config.tabs.general": "General",
  "config.tabs.generalHint": "Language and other preferences",
  "config.tabs.direct": "Direct",
  "config.tabs.directHint": "Raw per-channel faders — debug tool",

  "vdj.title": "VirtualDJ tempo bridge",
  "vdj.intro":
    "Polls the Network Control Plugin running inside VirtualDJ for the current track's BPM and drives the overall tempo override. Requires VDJ 2023+ with a Pro license.",
  "vdj.section.connection": "Connection",
  "vdj.section.behavior": "Tempo behaviour",
  "vdj.host": "Host",
  "vdj.hostHint": "Usually 127.0.0.1 — VirtualDJ on the same machine as this app.",
  "vdj.port": "Port",
  "vdj.portHint": "Whatever you set in the plugin's settings cogwheel (often 8080).",
  "vdj.bearer": "Bearer token",
  "vdj.bearerHint": "Optional. Leave empty if you didn't set an auth string in VDJ.",
  "vdj.interval": "Polling interval (ms)",
  "vdj.intervalHint": "How often to ask VDJ. 250 is a sensible default.",
  "vdj.enabled": "Enabled",
  "vdj.enabledHint": "When on, polls VDJ continuously and applies the BPM. Survives restart.",
  "vdj.save": "Save",
  "vdj.statusSection": "Status",
  "vdj.status.running": "Running",
  "vdj.status.stopped": "Stopped",
  "vdj.status.lastBpm": "Last BPM",
  "vdj.status.lastUpdate": "Last successful read",
  "vdj.status.never": "Never",
  "vdj.status.error": "Last error",
  "vdj.status.secondsAgo": "{secs}s ago",
  "vdj.errSave": "Save failed: {err}",
  "vdj.halve.enabled": "Halve BPM above threshold",
  "vdj.halve.enabledHint":
    "When the music goes above the threshold, run the rig at BPM/2. Classic DJ trick — electronic music at 130 BPM is chaotic on chasers, half-rate stays musical.",
  "vdj.halve.threshold": "Threshold (BPM)",
  "vdj.halve.thresholdHint":
    "Halving kicks in strictly above this value. 95 is a sensible default.",

  "loops.title": "Loops",
  "loops.meta": "{count} group(s)",
  "loops.intro":
    "Play chains of scenes that cycle automatically. Each group runs scene 1 → 2 → 3 → 1… while you focus on the show.",
  "loops.new": "+ New loop",
  "loops.empty": "No loop groups yet. Create one and drop scenes into it.",
  "loops.emptyScenes": "No scenes in this loop yet.",
  "loops.sceneCount": "{count} scene{plural}",
  "loops.startHint": "Start loop",
  "loops.stopHint": "Stop loop",
  "loops.deleteTitle": "Delete loop",
  "loops.deleteBody": 'Delete loop "{name}"?',
  "loops.hold": "Hold per scene (ms)",
  "loops.holdHint": "0 = use each scene's own fade + hold cycle.",
  "loops.missingScene": "⚠ deleted scene",
  "loops.moveUp": "Move up",
  "loops.moveDown": "Move down",
  "loops.removeFromList": "Remove from list",
  "loops.addPlaceholder": "+ Add scene…",
  "loops.repeatOption": "{name} (repeat)",
  "loops.addSceneGroup": "Scenes",
  "loops.addSnapshotGroup": "Snapshots",
  "loops.snapshotChipHint": "Snapshot — applies the whole captured look (values, FX, master)",
  "loops.holdDisabledHint": "Interval is driven by the Overall BPM while sync is on",
  "loops.syncBpm": "Sync to BPM",
  "loops.syncBpmHint":
    "Advance the playlist on the music: each entry lasts the selected subdivision at the Overall BPM",
  "loops.syncBpmOff": "Overall BPM is off — using hold/natural timing",
  "loops.subdivisionHint": "How long each entry holds, in musical time",
  "loops.sub.quarter": "1/16 note",
  "loops.sub.half": "1/8 note",
  "loops.sub.one": "1 beat",
  "loops.sub.two": "2 beats",
  "loops.sub.four": "1 bar",
  "loops.sub.eight": "2 bars",
  "scenes.list.activeLoopPrefix": "Loop: ",
  "scenes.list.activeLoopStop": "Stop",

  "general.title": "General",
  "general.language": "Language",
  "general.languageHint": "Applies immediately. Saved per user in this device.",
  "general.language.en": "English",
  "general.language.es": "Español",
  "general.showFile": "Show file",
  "general.showFileHint":
    "Create a new show, open one from disk, or save the current one to a chosen path.",

  "sync.title": "Sync",
  "sync.intro":
    "Share config across your machines via a private GitHub Gist. Each user uses their own PAT and their own gist — nothing shared by default. Outputs are excluded from push by default (hardware usually differs between Mac and Windows).",
  "sync.section.token": "GitHub PAT",
  "sync.token.saved": "PAT saved in the keychain.",
  "sync.token.delete": "Delete",
  "sync.token.deleteConfirm": "Delete the PAT from the keychain?",
  "sync.token.placeholder": "ghp_… (scope: gist)",
  "sync.token.save": "Save",
  "sync.token.saving": "Saving…",
  "sync.token.connectedAs": "Connected as {user}.",
  "sync.token.help":
    "Generate a PAT at github.com/settings/tokens with the 'gist' scope. The token is stored in the OS keychain (macOS Keychain / Windows Credential Manager) — never in a file.",
  "sync.section.settings": "Settings",
  "sync.machineLabel": "Label for this machine",
  "sync.machineLabel.placeholder": "e.g. mac-julio, windows-studio",
  "sync.gistId": "Gist id",
  "sync.gistId.placeholder": "leave empty so the first Push creates one",
  "sync.includeOutputs": "Include Outputs in sync (only if both machines use the same hardware)",
  "sync.saveSettings": "Save settings",
  "sync.section.status": "Status",
  "sync.status.lastPush": "Last push:",
  "sync.status.lastPull": "Last pull:",
  "sync.status.lastRemote": "Last remote version seen:",
  "sync.status.never": "never",
  "sync.status.from": "from {machine}",
  "sync.probe": "Check remote",
  "sync.probing": "Checking…",
  "sync.probe.empty": "Remote: empty",
  "sync.probe.ahead": "Remote: {time} · {machine} — ahead, you should Pull first",
  "sync.probe.upToDate": "Remote: {time} · {machine} — up to date",
  "sync.section.actions": "Actions",
  "sync.action.push": "Push",
  "sync.action.pushing": "Pushing…",
  "sync.action.forcePush": "Force push",
  "sync.action.forcePushTitle": "Overwrite the remote without checking if it's ahead",
  "sync.action.pull": "Pull",
  "sync.action.pulling": "Pulling…",
  "sync.action.pullConfirm":
    "Pull will overwrite the current show with the gist version. Continue?",
  "sync.backupHint":
    "Before each Pull the app saves a backup of the current show under sync-backups/. The newest file is your pre-pull show.",
  "sync.toast.pushOk": "Push OK — gist {id}…, remote {time}",
  "sync.toast.pullOk": "Pull OK — from {machine}, remote {time}. Backup in sync-backups/.",

  "remote.title": "Remote (mobile)",
  "remote.intro":
    "LAN bridge for the iPhone companion app. Serves scenes / master / blackout / blind over local WiFi. No TLS — only on trusted networks. On Windows the firewall usually asks for permission the first time it starts: accept at least 'private networks'.",
  "remote.status": "Status:",
  "remote.statusRunning": "running",
  "remote.statusStopped": "stopped",
  "remote.start": "Start",
  "remote.stop": "Stop",
  "remote.connectManually": "Connect manually:",
  "remote.clientsConnected": "Clients connected:",
  "remote.pairing": "Pairing",
  "remote.pairing.expiresIn": "Expires in {secs}s. Scan the QR from the app or type the PIN.",
  "remote.pairing.cancel": "Cancel",
  "remote.pairing.start": "Pair new device",
  "remote.devices": "Paired devices ({count})",
  "remote.devices.empty": "None yet.",
  "remote.devices.pairedAgo": "paired {ago}",
  "remote.devices.seenAgo": "· seen {ago}",
  "remote.devices.revoke": "Revoke",
  "remote.devices.revokeConfirm": "Revoke this device? It will lose the connection.",
  "remote.time.secAgo": "{n}s ago",
  "remote.time.minAgo": "{n}m ago",
  "remote.time.hAgo": "{n}h ago",
  "remote.time.dAgo": "{n}d ago",

  "common.cancel": "Cancel",
  "common.save": "Save",
  "common.delete": "Delete",
  "common.loading": "Loading…",
  "common.empty": "Empty",
  "common.add": "Add",
  "common.remove": "Remove",
  "common.edit": "Edit",
  "common.close": "Close",
  "common.refresh": "Refresh",
  "common.connect": "Connect",
  "common.disconnect": "Disconnect",
  "common.connected": "Connected",
  "common.disconnected": "Disconnected",
  "common.universe": "Universe",
  "common.address": "Address",
  "common.channel": "Channel",
  "common.fixture": "Fixture",
  "common.fixtures": "Fixtures",
  "common.mode": "Mode",
  "common.name": "Name",
  "common.value": "Value",
  "common.enabled": "Enabled",
  "common.disabled": "Disabled",
  "common.optional": "optional",
  "common.yes": "Yes",
  "common.no": "No",

  "direct.title": "DMX Control — Direct Output (universe 0)",
  "direct.fpsNone": "— FPS",
  "direct.fpsValue": "{fps} FPS",
  "direct.statsMeta": "frames {frames} · late {late} · max {ms} ms",
  "direct.master": "Master",
  "direct.blackoutOn": "Blackout ON",
  "direct.blackout": "Blackout",
  "direct.clearAll": "Clear all",

  "library.title": "Library ({count})",
  "library.search": "Search…",
  "library.reload": "Reload",
  "library.uploadImage": "Upload image",
  "library.changeImage": "Change image",
  "library.errorOpenDialog": "Could not open dialog: {err}",
  "library.errorSetImage": "Could not change image: {err}",
  "library.modeSummary": "{name} · {count}ch",
  "library.filesPath": "JSON files live in {path}.",
  "library.gobosToggle": "Edit gobos / colours",
  "library.gobosCollapse": "Collapse",
  "library.gobosEmpty":
    "This fixture has no channels with ranges (no gobo / colour wheel / macro defined).",
  "library.gobosModeFmt": "{name} ({count}ch)",
  "library.gobosChannelFmt": "{name} · {role}",
  "library.gobosUpload": "Upload",
  "library.gobosReplace": "Replace",
  "library.gobosRangeFmt": "{label} (DMX {from}–{to})",
  "library.gobosErrorOversize": "Image too large for a per-range thumbnail (max 512 KB).",

  "outputs.title": "Outputs",
  "outputs.kind.mock": "Mock (logging)",
  "outputs.kind.artNet": "Art-Net",
  "outputs.kind.sacn": "sACN (E1.31)",
  "outputs.kind.enttec": "Enttec DMX USB Pro",
  "outputs.kind.openDmx": "Open DMX (OS serial — fallback)",
  "outputs.kind.openDmxFtdi": "Open DMX / ElectroTAS (FTDI direct, recommended)",
  "outputs.add.mock": "+ Mock",
  "outputs.add.artNet": "+ Art-Net",
  "outputs.artnetScan.button": "🔍 Scan Art-Net",
  "outputs.artnetScan.hint":
    "Broadcast an ArtPoll and list every Art-Net node answering on the network",
  "outputs.artnetScan.scanning": "Scanning…",
  "outputs.artnetScan.title": "Art-Net nodes on the network",
  "outputs.artnetScan.rescan": "Rescan",
  "outputs.artnetScan.close": "Close",
  "outputs.artnetScan.inProgress": "Polling the network — nodes have ~2.5 s to answer…",
  "outputs.artnetScan.none":
    "No nodes answered. Check the interface/VLAN, that the node is powered, and that nothing is blocking UDP 6454.",
  "outputs.artnetScan.universesOut": "out: {list}",
  "outputs.artnetScan.add": "+ Output",
  "outputs.artnetScan.addHint":
    "Add an Art-Net output pre-filled with this node's IP and universes",
  "outputs.add.sacn": "+ sACN",
  "outputs.add.enttec": "+ Enttec USB",
  "outputs.add.openDmx": "+ Open DMX (OS serial)",
  "outputs.add.openDmxFtdi": "+ Open DMX (FTDI direct)",
  "outputs.rescan": "Re-scan ports",
  "outputs.field.target": "Target IP:Port",
  "outputs.field.sourceName": "Source name",
  "outputs.field.priority": "Priority",
  "outputs.field.serialPort": "Serial port",
  "outputs.field.ftdiDevice": "FTDI device (by serial)",
  "outputs.field.dtrHigh": "DTR high",
  "outputs.field.rtsHigh": "RTS high",
  "outputs.field.universes": "Universes (comma-separated)",
  "outputs.placeholder.select": "— select —",
  "outputs.tag.ftdi": "(FTDI)",
  "outputs.tag.offline": "(offline)",
  "outputs.tag.inUse": "(in use)",
  "outputs.zadigHint":
    "On Windows, libusb only sees FTDI devices bound to WinUSB. If your DMX FTDI doesn't show up, run Zadig once and pick the WinUSB driver for that interface (this removes the COM port). To keep the COM port, switch this output to 'Serial' and pick the FTDI from the port list.",
  "outputs.remove": "Remove",
  "outputs.empty": "No outputs. Add one above.",
  "outputs.dirty": "Unsaved changes",
  "outputs.ok": "OK",
  "outputs.discard": "Discard",
  "outputs.apply": "Apply",

  "patch.title": "Patch ({count} fixtures)",
  "patch.statusClean": "✓ patch clean",
  "patch.statusIssues": "{conflicts} conflicts · {problems} problems",
  "patch.qty": "Qty",
  "patch.universe": "U",
  "patch.address": "Addr",
  "patch.labelPlaceholder": "Label (optional)",
  "patch.add": "Add",
  "patch.modeFmt": "{name} ({count}ch)",
  "patch.col.id": "ID",
  "patch.col.label": "Label",
  "patch.col.fixture": "Fixture",
  "patch.col.mode": "Mode",
  "patch.col.universe": "U",
  "patch.col.address": "Addr",
  "patch.col.range": "Range",
  "patch.del": "Del",
  "patch.unknownDef": "?? {id}",
  "patch.conflictFmt": "overlap on universe {u} channels {start}…{end}",

  "buttons.title": "Global buttons",
  "buttons.subhead": "Fades in milliseconds · independent in/out",
  "buttons.section.master": "Master",
  "buttons.section.blackout": "Blackout",
  "buttons.section.blind": "Blind (halogen)",
  "buttons.masterIntro":
    "Channels the master fader scales. With no fixtures assigned = auto: every patched fixture gets its intensity (or RGB if no dimmer) scaled by the master, and pan/tilt/colour-wheel/etc. pass through untouched. Assign fixtures below to override the channel set per fixture (e.g. include W/A on RGBWAP pars).",
  "buttons.assignedMaster": "Fixtures assigned to Master ({count})",
  "buttons.assignedMasterAuto": "Fixtures assigned to Master (auto · {count})",
  "buttons.emptyMaster": "Patch fixtures first to assign them to master.",
  "buttons.masterHintAuto":
    "Auto: intensity (or R/G/B if no dimmer) is scaled. Other channels untouched.",
  "buttons.masterHintCustom": "Only these channels are scaled by the master fader.",
  "buttons.blackoutIntro":
    "Cross-fades to off the channels you pick on each fixture. With no fixtures assigned = auto mode: every patched fixture kills intensity (or RGB if no dimmer) + strobe; pan/tilt/zoom stay put so heads don't slam to the floor. To target something specific (kill only intensity, or also a custom macro channel) add the fixtures below and tick the channels that should go to 0.",
  "buttons.blindIntro":
    "Hold-to-flash with a cross-fade against each light's current state. Hold the button: assigned fixtures jump in with a quick fade-in to halogen colour; on release, slow fade-out back to the colour they had (green at 50% returns to green). By default blind writes warm-white over intensity + RGB. To activate a specific channel instead (strobe, shutter, custom function), tick it on the fixture row — that channel slams to 255 instead of the default halogen.",
  "buttons.fadeIn": "Fade in (ms)",
  "buttons.fadeOut": "Fade out (ms)",
  "buttons.assignedBlackout": "Fixtures assigned to Blackout ({count})",
  "buttons.assignedBlackoutAuto": "Fixtures assigned to Blackout (auto · {count})",
  "buttons.assignedBlind": "Fixtures assigned to Blind ({count})",
  "buttons.emptyBlackout": "Patch fixtures first to assign them to blackout.",
  "buttons.emptyBlind": "Patch fixtures first to assign them to blind.",
  "buttons.blackoutHintAuto":
    "Auto: intensity (or RGB if not present) + strobe → 0. Pan/tilt untouched.",
  "buttons.blackoutHintCustom": "Only these channels go to 0.",
  "buttons.blindHintAuto": "Halogen default (warm-white over intensity + RGB).",
  "buttons.blindHintCustom": "Only these channels are slammed to 255 (no halogen).",

  "midi.title": "MIDI",
  "midi.intro":
    "Connect a control surface (e.g. Launchpad MK2). The first matching device is auto-connected at launch; you can override here.",
  "midi.devices": "Devices",
  "midi.refresh": "Refresh",
  "midi.connect": "Connect",
  "midi.disconnect": "Disconnect",
  "midi.connected": "Connected to {name}",
  "midi.notConnected": "Not connected.",
  "midi.lastEvent": "Last event: {event}",
  "midi.colorTest": "Color test",
  "midi.empty": "No MIDI devices found.",
  "midi.refreshHint":
    "Connect a MIDI controller (Launchpad MK2, etc.) to use as a surface. When you connect a Launchpad, the surface controller takes the bottom row (8 chasers, one per pad) and the scene buttons on the right side (red = blackout, white = momentary blind).",
  "midi.detectedDevices": "Detected devices ({count})",
  "midi.errListing": "Could not list MIDI devices: {err}",
  "midi.errConnect": "Could not connect to {name}: {err}",
  "midi.errDisconnect": "Could not disconnect: {err}",
  "midi.errTest": "Pad test failed: {err}",
  "midi.in": "in",
  "midi.out": "out",
  "midi.bothIO": "in + out",
  "midi.onlyIn": "in only",
  "midi.surfaceActive": "surface active",
  "midi.testPads": "Test pads (Launchpad MK2)",
  "midi.statusSection": "Status",
  "midi.connectedToFmt": "Connected to {name} ({io}){surface}",

  "streamdeck.title": "Stream Deck",
  "streamdeck.intro":
    "Plug in an Elgato Stream Deck and the surface controller takes over the keys: chasers on row 1, movements on row 2, scenes on row 3 plus blind (momentary) and blackout (toggle). Layout is fixed for now — first 5 chasers/movements and first 3 scenes.",
  "streamdeck.refresh": "Refresh",
  "streamdeck.connect": "Connect",
  "streamdeck.disconnect": "Disconnect",
  "streamdeck.empty": "No Stream Decks found.",
  "streamdeck.detectedDevices": "Detected devices ({count})",
  "streamdeck.deviceMeta": "{keys} keys · {serial}",
  "streamdeck.statusSection": "Status",
  "streamdeck.connectedToFmt": "Connected to {kind} ({keys} keys) · surface active",
  "streamdeck.errListing": "Could not list Stream Decks: {err}",
  "streamdeck.errConnect": "Could not connect to {name}: {err}",
  "streamdeck.errDisconnect": "Could not disconnect: {err}",

  "ai.title": "AI",
  "ai.intro":
    "Provider, model and API key for LLM scene generation. Keys are kept in the OS app-config dir, off the show file.",
  "ai.provider": "Provider",
  "ai.provider.anthropic": "Anthropic (Claude)",
  "ai.provider.openai": "OpenAI",
  "ai.apiKey": "API key",
  "ai.apiKey.show": "Show",
  "ai.apiKey.hide": "Hide",
  "ai.model": "Model",
  "ai.test": "Test connection",
  "ai.testing": "Testing…",
  "ai.testOk": "Connection OK.",
  "ai.testFail": "Connection failed: {err}",
  "ai.save": "Save",
  "ai.saved": "Saved.",
  "ai.refreshModels": "Refresh models",
  "ai.subhead": "LLM scene generation (Anthropic / OpenAI) — POC",
  "ai.warning":
    "API keys are stored as plain text in the OS config dir (off the show file). In production they should move to the OS keychain.",
  "ai.providerActive": "Active provider",
  "ai.provider.none": "Disabled",
  "ai.provider.anthropicLong": "Anthropic (Claude)",
  "ai.provider.openaiLong": "OpenAI (GPT)",
  "ai.section.anthropic": "Anthropic",
  "ai.section.openai": "OpenAI",
  "ai.keyHint.anthropic": "Get an API key at",
  "ai.keyHint.openai": "Get an API key at",
  "ai.toggleKey.show": "Show key",
  "ai.toggleKey.hide": "Hide key",
  "ai.testDisabledHint": "Choose a provider and fill in the API key first",
  "ai.testActiveHint": "Make a minimal request to verify the API key + model",
  "ai.savedToast": "Configuration saved.",
  "ai.errPrefix": "Error: {err}",

  "scenes.title": "Scenes",
  "scenes.metaSummary": "Multi-step + FX capture · {count} scene{plural}",
  "scenes.aiTrigger": "✨ Generate with AI",
  "scenes.aiTriggerHint": "Generate a fresh scene with AI from a prompt",
  "scenes.errCreate": "Could not create the scene: {err}",
  "scenes.list.new": "+ New scene",
  "scenes.list.empty": 'No scenes yet. Build a look in Stage and tap "New scene" to record it.',
  "scenes.list.lpHint": "LP row 3, pad {pad}",
  "scenes.list.recallHint": "Recall (▶ GO)",
  "scenes.list.stepCount": "{count} step{plural}",
  "scenes.list.activePrefix": "▶ {name}",
  "scenes.list.activeStep": " · step {step}",
  "scenes.list.release": "Release",
  "scenes.editor.empty": "Pick a scene from the left — or create a new one.",
  "scenes.editor.recallHint": "Recall this scene (▶ GO)",
  "scenes.editor.go": "▶ GO",
  "scenes.editor.cycleTotal": "Total cycle: {seconds}s",
  "scenes.editor.aiIterate": "✨ Improve with AI",
  "scenes.editor.aiIterateHint":
    "Iterate this scene with AI (refine values, fades, add/remove steps)",
  "scenes.editor.delete": "Delete",
  "scenes.editor.deleteConfirm": 'Delete "{name}"?',
  "scenes.editor.fxHint":
    "Each step records the chaser and movement active at that moment. Releasing the scene restores whatever was running before recall. Edit the FX state per step below.",
  "scenes.editor.stepsHeading": "Steps ({count})",
  "scenes.editor.stepsEmpty": "The scene has no steps. Add one from the block below.",
  "scenes.editor.addStepHeading": "Add step from current state",
  "scenes.editor.fadeIn": "Fade in (ms)",
  "scenes.editor.hold": "Hold (ms)",
  "scenes.editor.touchedOnly": "Touched only ({count})",
  "scenes.editor.addStep": "+ Add step",
  "scenes.editor.addDisabledHint": "Touch fixtures in Stage first",
  "scenes.editor.addEnabledHint": "Capture the current state as a new step",
  "scenes.editor.loopHint":
    "Steps play in order and loop back to the first when they finish. The next step starts when the current one's hold ends.",
  "scenes.editor.fixturesHint":
    '{writes} total writes over {fixtures} patched fixtures. Tip: if needed, Clear the programmer and use "Touched only" to iterate steps without overwriting.',
  "scenes.editor.clearProg": "Clear programmer",
  "scenes.programmer.label": "PROG · {count} fixture{plural} touched",
  "scenes.programmer.clear": "Clear",
  "snapshots.heading": "Snapshots",
  "snapshots.capture": "● Record snapshot",
  "snapshots.captureHint":
    "Freeze the whole current look: DMX values, master, chaser, movement, active scene/loop, blackout and BPM",
  "snapshots.empty": "No snapshots yet. Record one to freeze the current look.",
  "snapshots.activateHint":
    "Apply this snapshot. The current state is remembered — deactivating brings it back untouched.",
  "snapshots.deactivate": "Deactivate",
  "snapshots.deactivateHint": "Return to the state from before the snapshot was activated",
  "snapshots.recaptureHint": "Overwrite this snapshot with the current rig state",
  "snapshots.deleteConfirm": 'Delete snapshot "{name}"?',
  "snapshots.deleteHint": "Delete snapshot",
  "snapshots.activeBadge": "ACTIVE",
  "scenes.step.fade": "Fade",
  "scenes.step.hold": "Hold",
  "scenes.step.ms": "ms",
  "scenes.step.metaFmt": "{fixtures}f · {channels}ch",
  "scenes.step.placeholder": "Step {n}",
  "scenes.step.update": "⟳",
  "scenes.step.updateAllHint":
    "Re-record this step with the rig's current state (all its fixtures)",
  "scenes.step.updateTouched": "⟳T",
  "scenes.step.updateTouchedHint": "Re-record only the touched fixtures (the rest stays as-is)",
  "scenes.step.removeHint": "Remove this step",
  "scenes.step.removeOnlyHint": "Cannot remove the only step",
  "scenes.step.removeConfirm": "Remove step {n}?",
  "scenes.fx.chaser": "Chaser",
  "scenes.fx.movement": "Movement",
  "scenes.fx.inherit": "Don't touch",
  "scenes.fx.disable": "Disable",
  "scenes.fx.enable": "Enable:",
  "scenes.fx.noOptions": "— no options —",

  "chaser.title": "Ambient Chaser ({count})",
  "chaser.addExample": "+ Example chasers",
  "chaser.addNew": "+ New chaser",
  "chaser.empty":
    "No chasers. Create one and assign fixtures, or try + Example chasers to start with presets already configured.",
  "chaser.exampleNoticeNoFixtures":
    "Added {count} example chasers. Patch fixtures and add them to their slots to see them in action.",
  "chaser.exampleNoticeWithFixtures":
    "Added {count} example chasers with your {fixtures} already-patched fixture(s).",
  "chaser.toggle.on": "ON",
  "chaser.toggle.off": "OFF",
  "chaser.toggle.disable": "Disable chaser",
  "chaser.toggle.enable": "Enable chaser",
  "chaser.lpHint": "Launchpad pad {pad} (bottom row)",
  "chaser.summary": "{bpm} BPM · {slots} slots",
  "chaser.summary.fade": " · fade",
  "chaser.hide": "Hide",
  "chaser.edit": "Edit",
  "chaser.del": "Del",
  "chaser.tab.pattern": "Pattern & Color",
  "chaser.tab.timing": "Timing & Fade",
  "chaser.tab.slots": "Slots ({count})",
  "chaser.section.pattern": "Pattern",
  "chaser.section.colorMode": "Colour mode",
  "chaser.section.tempo": "Tempo",
  "chaser.section.levels": "Levels",
  "chaser.section.fade": "Fade between steps",
  "chaser.bpm": "BPM",
  "chaser.subdivision": "Subdivision",
  "chaser.master": "Master",
  "chaser.background": "Background",
  "chaser.fadeEnabled": "Enabled",
  "chaser.fadeAmount": "Amount (% of step)",
  "chaser.fadeCurve": "Curve",
  "chaser.fadeHint":
    "0% = snap (off→on instantaneous). 90% = almost the whole step crossfading. The curve defines the shape of the transition.",
  "chaser.preview.empty": "No slots — add fixtures to see the effect.",
  "chaser.slots.empty": "No slots — add fixtures below.",
  "chaser.slots.intLabel": "Int",
  "chaser.slots.colorLabel": "Color",
  "chaser.slots.add": "+ Add slot",
  "chaser.slots.fixtureFmt": "{label} (U{universe}/{address})",
  "chaser.cadence": "Cadence",
  "chaser.cadenceN": "N steps",
  "chaser.rotation": "Rotation",
  "chaser.removeColor": "Remove colour",
  "chaser.addColor": "+ Add colour",
  "chaser.rainbowSpeed": "Speed (deg/step)",
  "chaser.rainbowSpread": "Spread (rainbow length)",
  "chaser.fadeCurveLabel.linear": "Linear",
  "chaser.fadeCurveLabel.easeInOut": "Ease in/out",
  "chaser.fadeCurveLabel.easeIn": "Ease in",
  "chaser.fadeCurveLabel.easeOut": "Ease out",
  "chaser.fadeCurveLabel.exponential": "Exponential",
  "chaser.fadeCurveLabel.logarithmic": "Logarithmic",
  "chaser.subdivisionLabel.16": "1/16",
  "chaser.subdivisionLabel.8": "1/8",
  "chaser.subdivisionLabel.4": "1/4",
  "chaser.subdivisionLabel.2": "1/2",
  "chaser.subdivisionLabel.1": "1/1",
  "chaser.patternLabel.allTogether": "All together",
  "chaser.patternLabel.alternate": "Alternate",
  "chaser.patternLabel.chase": "Chase →",
  "chaser.patternLabel.chaseReverse": "Chase ←",
  "chaser.patternLabel.pingPong": "Ping-pong",
  "chaser.patternLabel.wave": "Wave →",
  "chaser.patternLabel.waveReverse": "Wave ←",
  "chaser.patternLabel.build": "Build",
  "chaser.patternLabel.buildReverse": "Build reverse",
  "chaser.patternLabel.centerOut": "Center out",
  "chaser.patternLabel.symmetric": "Symmetric",
  "chaser.patternLabel.random": "Random",
  "chaser.patternLabel.outsideIn": "Outside in",
  "chaser.patternLabel.invertedChase": "Inverted chase (shadow)",
  "chaser.patternLabel.groupsOfTwo": "Groups of two",
  "chaser.patternLabel.groupsOfThree": "Groups of three",
  "chaser.patternLabel.halfSwap": "Half swap",
  "chaser.patternLabel.edges": "Edges only",
  "chaser.patternLabel.pulseOut": "Pulse outward (sonar)",
  "chaser.patternLabel.pulseIn": "Pulse inward",
  "chaser.patternLabel.accordion": "Accordion (breathing)",
  "chaser.patternLabel.bowtie": "Bowtie (build from edges)",
  "chaser.patternLabel.dualChase": "Dual chase (paired heads)",
  "chaser.patternLabel.symmetricBounce": "Symmetric bounce",
  "chaser.cadenceLabel.everyStep": "Every step (A B A B…)",
  "chaser.cadenceLabel.everyN": "Every N steps (AAAA BBBB)",
  "chaser.cadenceLabel.perSlot": "Per slot (half A, half B)",
  "chaser.cadenceLabel.alternateSlots": "Alternate slots (zebra)",
  "chaser.cadenceLabel.chasePerColor": "One chase A, next chase B",
  "chaser.rotationLabel.perStep": "Per step",
  "chaser.rotationLabel.perCycle": "Per cycle (every chase)",
  "chaser.rotationLabel.perSlot": "Per slot (static)",
  "chaser.colorModeLabel.disabled": "Disabled (intensity only)",
  "chaser.colorModeLabel.single": "Single colour",
  "chaser.colorModeLabel.twoColor": "Two-colour cadence",
  "chaser.colorModeLabel.palette": "Palette",
  "chaser.colorModeLabel.rainbow": "Rainbow",

  "movement.title": "Movement Generators",
  "movement.addNew": "+ Add movement",
  "movement.empty":
    "No movements. Create one to start — the first maps to Launchpad pad 21 (row 2). Subsequent ones take the 7 pads to its right.",
  "movement.delete": "Delete",
  "movement.deleteConfirm": 'Delete "{name}"?',
  "movement.lpHint": "Launchpad pad {pad} (row 2)",
  "movement.legend.fixtures": "Fixtures ({count})",
  "movement.legend.shape": "Shape",
  "movement.legend.canon": "Canon",
  "movement.legend.timing": "Timing",
  "movement.legend.transform": "Transform",
  "movement.fixtures.empty": "No fixtures assigned.",
  "movement.fixtures.invertPan": "Inv P",
  "movement.fixtures.invertTilt": "Inv T",
  "movement.fixtures.add": "+ Add fixture…",
  "movement.fixtures.fixtureFmt": "{label} (U{universe}/{address})",
  "movement.shape.sides": "Sides",
  "movement.shape.points": "Points",
  "movement.shape.innerRatio": "Inner ratio",
  "movement.timing.bpm": "BPM",
  "movement.timing.loopLength": "Loop length",
  "movement.transform.sizeX": "Size X",
  "movement.transform.sizeY": "Size Y",
  "movement.transform.centerX": "Center X",
  "movement.transform.centerY": "Center Y",
  "movement.transform.rotation": "Rotation",
  "movement.transform.reset": "Reset transform",
  "movement.previewHint":
    "Sub-phase A: only Circle. More shapes (Polygon, Star, Lissajous, Sine combos…) in sub-phases C and D.",
  "movement.previewBadgeOff": "OFF",
  "movement.previewMeta": "{count} fixtures · phase {phase}%",
  "movement.previewPaused": "{count} fixtures · paused",
  "movement.wave.pan": "Pan",
  "movement.wave.tilt": "Tilt",
  "movement.wave.waveform": "Waveform",
  "movement.wave.frequency": "Frequency",
  "movement.wave.phaseShift": "Phase shift",
  "movement.wave.amplitude": "Amplitude",
  "movement.wave.offset": "Offset",
  "movement.preset.label": "Presets:",
  "movement.preset.loadHint": "Load {name} preset",
  "movement.shapeLabel.circle": "Circle",
  "movement.shapeLabel.polygon": "Polygon",
  "movement.shapeLabel.star": "Star",
  "movement.shapeLabel.figureEight": "Figure 8",
  "movement.shapeLabel.lineH": "Line ⇄",
  "movement.shapeLabel.lineV": "Line ⇅",
  "movement.shapeLabel.sineCombo": "Sine combo",
  "movement.spreadLabel.none": "None (all in phase)",
  "movement.spreadLabel.even": "Even (canon)",
  "movement.spreadLabel.symmetric": "Symmetric",
  "movement.spreadLabel.pairs": "Pairs",
  "movement.spreadLabel.manual": "Manual",
  "movement.directionLabel.forward": "Forward →",
  "movement.directionLabel.reverse": "Reverse ←",
  "movement.directionLabel.pingPong": "Ping-pong ↔",
  "movement.subdivLabel.16": "1/16 (very fast)",
  "movement.subdivLabel.8": "1/8",
  "movement.subdivLabel.1beat": "1 beat",
  "movement.subdivLabel.2beats": "2 beats",
  "movement.subdivLabel.4beats": "4 beats",
  "movement.subdivLabel.8beats": "8 beats",
  "movement.waveformLabel.sine": "Sine",
  "movement.waveformLabel.cosine": "Cosine",
  "movement.waveformLabel.triangle": "Triangle",
  "movement.waveformLabel.square": "Square",
  "movement.waveformLabel.sawtooth": "Sawtooth",
  "movement.waveformLabel.rampUp": "Ramp ↑",
  "movement.waveformLabel.rampDown": "Ramp ↓",
  "movement.presetLabel.circle": "Circle",
  "movement.presetLabel.figureEight": "Figure 8 (1:2)",
  "movement.presetLabel.lissajous32": "Lissajous 3:2",
  "movement.presetLabel.lissajous54": "Lissajous 5:4",
  "movement.presetLabel.wavePan": "Wave (pan only)",

  "stage.title": "Stage",
  "stage.meta": "{count} fixtures · grid {grid}px",
  "stage.metaSelected": " · {count} selected",
  "stage.empty": "No fixtures.",
  "stage.sidebar.placeholder":
    "Select a fixture or a type to open its encoders. ⌘/Ctrl-click adds or removes from the selection.",
  "stage.fixture.untouchAria": "Remove from touched ({count} channels)",
  "stage.fixture.untouchTitle": "Click to remove from touched · channels: {labels}",
  "stage.type.selectHint": "Select the {count} units (⌘/Ctrl-click to add)",
  "stage.editor.unavailable": "Definition not available.",
  "stage.editor.multiHeading": "{fixtures} fixtures · {types} types",
  "stage.editor.multiBadge": "×{count}",
  "stage.editor.mixedHint": "Showing only the controls common to every selected unit.",
  "stage.editor.activeFx": "Active effects",
  "stage.editor.fxKindChaser": "Chaser",
  "stage.editor.fxKindMove": "Move",
  "stage.editor.fxTooltipChaser": "Chaser: {name} · affects {touches}/{total}",
  "stage.editor.fxTooltipMovement": "Movement: {name} · affects {touches}/{total}",
  "stage.editor.center": "Center",
  "stage.editor.home": "Home",
  "stage.editor.section.intStrobe": "Intensity & strobe",
  "stage.editor.section.color": "Color",
  "stage.editor.section.extras": "Extras",
  "stage.fxbar.scenes": "Scenes",
  "stage.fxbar.snapshots": "Snapshots",
  "stage.fxbar.movements": "Movements",
  "stage.fxbar.chasers": "Chasers",
  "stage.fxbar.releaseHint": "Release — the rig stays in its current state",
  "stage.fxbar.idle": "— no scene active",
  "stage.fxbar.openScenes": "Open scenes panel",
  "stage.fxbar.closeScenes": "Close scenes panel",
  "stage.fxbar.scenesBtnClose": "Close",
  "stage.fxbar.scenesBtnOpen": "Scenes…",
  "stage.fxbar.morePill": "+{n} more",
  "stage.fxbar.scenePillTitle": "▶ {name} ({count} step{plural})",
  "stage.fxbar.disableMovement": "Disable {name}",
  "stage.fxbar.enableMovement": "Enable {name} ({slots} slots)",
  "stage.fxbar.disableChaser": "Disable {name}",
  "stage.fxbar.enableChaser": "Enable {name} ({slots} slots)",
  "stage.fxbar.stepName": "{name}",
  "stage.fxbar.stepFmt": "step {step}/{total}",
  "stage.sqp.aria": "Scene panel",
  "stage.sqp.title": "Scenes",
  "stage.sqp.metaFmt": "{scenes} · {touched} touched",
  "stage.sqp.list": "List",
  "stage.sqp.newScene": "+ New",
  "stage.sqp.empty": "No scenes. Build a look and tap + New to record it.",
  "stage.sqp.recallTitle": "Recall ({count} step{plural})",
  "stage.sqp.editSceneTitle": "Edit steps for this scene",
  "stage.sqp.sceneStepFmt": "{count} step{plural}",
  "stage.sqp.sceneLive": " · live",
  "stage.sqp.releaseActive": "Release active scene",
  "stage.sqp.stepsHeading": "Steps for {name}",
  "stage.sqp.tagLive": "LIVE",
  "stage.sqp.tagStopped": "STOPPED",
  "stage.sqp.followActive": "Follow active",
  "stage.sqp.followActiveHint": "Resume following the active scene",
  "stage.sqp.stepPlaceholder": "Step {n}",
  "stage.sqp.fadeTitle": "Fade in (ms)",
  "stage.sqp.holdTitle": "Hold (ms)",
  "stage.sqp.overwriteAll": "Overwrite this step with the rig's current state",
  "stage.sqp.overwriteTouched": "Overwrite only the touched fixtures",
  "stage.sqp.removeStep": "Remove step",
  "stage.sqp.removeOnlyHint": "Cannot remove the only step",
  "stage.sqp.recordHeadingNew": "Record new scene",
  "stage.sqp.recordHeadingAdd": 'Add step to "{name}"',
  "stage.sqp.fade": "Fade",
  "stage.sqp.hold": "Hold",
  "stage.sqp.ms": "ms",
  "stage.sqp.touchedOnly": "Touched only ({count})",
  "stage.sqp.addStep": "+ Add step",
  "stage.sqp.recordNew": "● Record",
  "stage.sqp.touchedHelperOn": "Hide touched halos in the canvas",
  "stage.sqp.touchedHelperOff": "Show touched halos in the canvas while this panel is open",
  "stage.sqp.touchedToggle": "👁 Touched",
  "stage.sqp.locateHint": "Pulse the touched fixtures to find them on the canvas",
  "stage.sqp.locate": "📍 Locate",
  "stage.sqp.clearProg": "Clear PROG",
  "stage.sqp.errRecord": "Could not record: {err}",
  "stage.sqp.errAddStep": "Could not add the step: {err}",
  "stage.sqp.errCreate": "Could not create: {err}",
  "stage.sqp.errNoTouched": "No fixtures touched; move a slider to mark them.",
  "stage.menu.fixtureSingular": "Fixture",
  "stage.menu.fixturePlural": "{count} fixtures",
  "stage.menu.centerPanTilt": "Center Pan/Tilt",
  "stage.menu.park": "Park (defaults)",
  "stage.menu.fullIntensity": "Full intensity",
  "stage.menu.blackoutFixture": "Blackout (intensity 0)",
  "stage.menu.rename": "Rename…",
  "stage.menu.duplicate": "Duplicate",
  "stage.menu.untouch": "Untouch",
  "stage.menu.remove": "Remove",
  "stage.confirm.removeOne": 'Remove "{name}"?',
  "stage.confirm.removeMany": "Remove {count} fixtures?",

  "aiGen.aria": "Generate scene with AI",
  "aiGen.title": "Generate scene with AI",
  "aiGen.titleIterating": "Iterating: {name}",
  "aiGen.errPromptEmpty": "Write a prompt — e.g. 'warm amber with slow fade'.",
  "aiGen.errScopeEmpty": "Tick at least one fixture in the list or switch the scope to 'all'.",
  "aiGen.errRefineEmpty": "Write what you want to change (e.g. 'step 2 faster').",
  "aiGen.field.prompt": "Prompt",
  "aiGen.field.promptPlaceholder":
    "e.g. 'sync warm amber pulses, fade 600ms, hold 400ms' · 'slow cold blue sweep left to right' · 'punk red alternating strobe'",
  "aiGen.field.stepCount": "Step count",
  "aiGen.field.scope": "Scope",
  "aiGen.scope.all": "All fixtures ({count})",
  "aiGen.scope.selected": "Selected only",
  "aiGen.scope.fixturesHint":
    "Tick the fixtures you want the AI to move. The rest stay as they are.",
  "aiGen.disabledHint": "Add fixtures to the patch first",
  "aiGen.activeHint": "Generate scene (may take 5-15 seconds)",
  "aiGen.generating": "Generating…",
  "aiGen.generate": "✨ Generate",
  "aiGen.preview.stepCount": "{count} step{plural}",
  "aiGen.preview.stepPlaceholder": "Step {n}",
  "aiGen.preview.stepMeta": "fade {fade}ms · hold {hold}ms · {count} fixture{plural}",
  "aiGen.refine": "Refine",
  "aiGen.refinePlaceholder":
    "e.g. 'step 2 faster' · 'less blue, more amber' · 'add strobe on the last one'",
  "aiGen.refineHint": "Apply the change on top of the current draft without losing the good parts",
  "aiGen.refining": "Thinking…",
  "aiGen.refineLabel": "↻ Refine",
  "aiGen.resetFromScratch": "Start over",
  "aiGen.replaceOriginal": "Replace original",
  "aiGen.replaceOriginalHint": 'Overwrite the steps of "{name}" with this draft',
  "aiGen.replacing": "Replacing…",
  "aiGen.applyNew": "Apply as new scene",
  "aiGen.applying": "Applying…",

  "p3d.types.title": "Per fixture type ({count})",
  "p3d.types.intro":
    "Brightness, beam angle and prism are configured per fixture model — every instance of the same type shares these values.",
  "p3d.types.metaPrefix": "{count}× · brightness {brightness}",
  "p3d.types.brightness": "Brightness",
  "p3d.types.brightnessHint":
    "Multiplies the rendered intensity (beam + spotlight + halo). Default 1.0.",
  "p3d.types.beamOverride": "Beam angle override",
  "p3d.types.beamMin": "Min °",
  "p3d.types.beamMax": "Max °",
  "p3d.types.beamMinHint": "Half-angle of the beam at closed zoom",
  "p3d.types.beamMaxHint": "Half-angle of the beam at open zoom",
  "p3d.types.prismOverride": "Prism override",
  "p3d.types.prismThreshold": "Threshold",
  "p3d.types.prismFacets": "Facets",
  "p3d.types.prismSplay": "Splay °",
  "p3d.types.prismThresholdHint": "DMX value above which the prism activates",
  "p3d.types.prismFacetsHint": "Total number of beams (including the central one)",
  "p3d.types.prismSplayHint": "Angular spacing between beams",
  "p3d.types.reset": "Remove overrides (back to defaults)",
};

export const es: Translation = {
  "app.brand": "DMX Control",
  "app.tab.stage": "Stage",
  "app.tab.scenes": "Escenas",
  "app.tab.loops": "Loops",
  "app.tab.chaser": "Chaser",
  "app.tab.movement": "Movimiento",
  "app.tab.preview3d": "Preview 3D",
  "app.tab.config": "Config",
  "app.openInOtherWindow": "Abrir {name} en otra ventana",
  "app.fullscreen": "Pantalla completa",
  "app.fullscreenHint": "Pantalla completa (F11)",
  "app.button.new": "Nuevo",
  "app.button.open": "Abrir…",
  "app.button.save": "Guardar",
  "app.global.blackout": "BLACKOUT",
  "app.global.blackoutTitle": "Blackout (toggle, fades configurables)",
  "app.global.blind": "BLIND",
  "app.global.blindTitle": "Blind / blinder (mantené presionado, halógeno con fade in/out)",
  "app.global.bpm.label": "BPM",
  "app.global.bpm.tap": "TAP",
  "app.global.bpm.tapHint":
    "Tocá varias veces para fijar el BPM global a tu ritmo. Después de dos taps el valor se actualiza y el override se activa.",
  "app.global.bpm.on": "ON",
  "app.global.bpm.off": "OFF",
  "app.global.bpm.toggleHint":
    "Cuando está en ON, todos los chasers y movimientos corren a este BPM en vez de al tempo propio que tengan configurado.",
  "app.global.bpm.editHint": "Escribí un BPM (20–300) y presioná Enter para fijarlo.",
  "app.global.bpm.recordPattern": "REC",
  "app.global.bpm.recordPatternHint":
    "Grabá un ritmo sincopado (clave, chacha…). Fijá primero el BPM, después tocá REC y golpeá el patrón. Tocá de nuevo para confirmar.",
  "app.global.bpm.recordingPattern": "STOP",
  "app.global.bpm.recordingHint": "Grabando… tocá el patrón y presioná para confirmar.",
  "app.global.bpm.clearPattern": "✕",
  "app.global.bpm.clearPatternHint": "Borrá el patrón activo; los chasers vuelven al BPM normal.",
  "app.global.bpm.patternActive": "{count} golpes / {bars} compás(es)",
  "app.show.untitled": "Sin título",
  "app.show.unsaved": "(sin guardar) · click para renombrar",
  "app.show.renameHint": "{path} · click para renombrar",
  "app.show.renameHintEmpty": "(sin guardar) · click para renombrar",
  "app.dialog.closeTitle": "Cerrar aplicación",
  "app.dialog.closeBody":
    "¿Cerrar DMX Control?\nSe cerrarán también todas las ventanas popout abiertas.",
  "app.toast.savedAt": "Guardado en {path}",
  "app.toast.saveError": "Error al guardar: {err}",
  "app.toast.renameError": "No se pudo renombrar: {err}",

  "config.tabs.library": "Library",
  "config.tabs.libraryHint": "Definiciones de fixtures",
  "config.tabs.outputs": "Outputs",
  "config.tabs.outputsHint": "Universos y drivers DMX",
  "config.tabs.patch": "Patch",
  "config.tabs.patchHint": "Asignar fixtures a direcciones DMX",
  "config.tabs.blackoutBlind": "Globales",
  "config.tabs.blackoutBlindHint":
    "Master, blackout y blind — canales afectados por cada uno por fixture",
  "config.tabs.bindings": "Botones",
  "config.tabs.bindingsHint": "Asignaciones personalizadas de botones del Launchpad + Stream Deck",
  "config.tabs.midi": "MIDI",
  "config.tabs.midiHint": "Superficie de control (Launchpad, etc.)",
  "config.tabs.streamdeck": "Stream Deck",
  "config.tabs.streamdeckHint": "Elgato Stream Deck como superficie de control",
  "config.tabs.ai": "IA",
  "config.tabs.aiHint": "Provider, modelo y API key para generación de escenas con LLM",
  "config.tabs.remote": "Remote",
  "config.tabs.remoteHint": "Bridge LAN para la app companion en iPhone",
  "config.tabs.sync": "Sync",
  "config.tabs.syncHint": "Compartir config entre máquinas via Gist privado de GitHub",
  "config.tabs.vdj": "VirtualDJ",
  "config.tabs.vdjHint": "Traer el BPM desde el Network Control Plugin de VirtualDJ",
  "config.tabs.general": "General",
  "config.tabs.generalHint": "Idioma y otras preferencias",
  "config.tabs.direct": "Direct",
  "config.tabs.directHint": "Faders crudos por canal — herramienta de debug",

  "vdj.title": "Puente de tempo con VirtualDJ",
  "vdj.intro":
    "Pollea al Network Control Plugin de VirtualDJ por el BPM de la canción actual y lo aplica como tempo global. Requiere VDJ 2023+ con licencia Pro.",
  "vdj.section.connection": "Conexión",
  "vdj.section.behavior": "Comportamiento del tempo",
  "vdj.host": "Host",
  "vdj.hostHint": "Normalmente 127.0.0.1 — VirtualDJ corriendo en la misma máquina.",
  "vdj.port": "Puerto",
  "vdj.portHint": "El que pusiste en el engranaje del plugin (típicamente 8080).",
  "vdj.bearer": "Bearer token",
  "vdj.bearerHint": "Opcional. Dejalo vacío si no configuraste auth en VDJ.",
  "vdj.interval": "Intervalo de poll (ms)",
  "vdj.intervalHint": "Cada cuánto preguntar a VDJ. 250 es un default razonable.",
  "vdj.enabled": "Habilitado",
  "vdj.enabledHint":
    "Cuando está prendido, polea a VDJ continuamente y aplica el BPM. Persiste tras reiniciar.",
  "vdj.save": "Guardar",
  "vdj.statusSection": "Estado",
  "vdj.status.running": "Corriendo",
  "vdj.status.stopped": "Detenido",
  "vdj.status.lastBpm": "Último BPM",
  "vdj.status.lastUpdate": "Última lectura exitosa",
  "vdj.status.never": "Nunca",
  "vdj.status.error": "Último error",
  "vdj.status.secondsAgo": "hace {secs}s",
  "vdj.errSave": "Falló al guardar: {err}",
  "vdj.halve.enabled": "Dividir BPM al medio cuando supera el umbral",
  "vdj.halve.enabledHint":
    "Cuando la música supera el umbral, manda al rig BPM/2. Truco clásico de DJ — electrónica a 130 BPM hace que las luces parezcan strobe, a la mitad respiran con el compás.",
  "vdj.halve.threshold": "Umbral (BPM)",
  "vdj.halve.thresholdHint":
    "El recorte se activa estrictamente por encima de este valor. 95 es un default razonable.",

  "loops.title": "Loops",
  "loops.meta": "{count} lista(s)",
  "loops.intro":
    "Reproducí cadenas de escenas que ciclan solas. Cada lista corre escena 1 → 2 → 3 → 1… mientras vos atendés el show.",
  "loops.new": "+ Nueva lista",
  "loops.empty": "Todavía no hay listas. Creá una y arrastrá escenas adentro.",
  "loops.emptyScenes": "Sin escenas en esta lista todavía.",
  "loops.sceneCount": "{count} escena{plural}",
  "loops.startHint": "Iniciar loop",
  "loops.stopHint": "Detener loop",
  "loops.deleteTitle": "Eliminar lista",
  "loops.deleteBody": "¿Eliminar la lista «{name}»?",
  "loops.hold": "Tiempo por escena (ms)",
  "loops.holdHint": "0 = usa el ciclo natural de cada escena (fade + hold).",
  "loops.missingScene": "⚠ escena eliminada",
  "loops.moveUp": "Mover arriba",
  "loops.moveDown": "Mover abajo",
  "loops.removeFromList": "Quitar de la lista",
  "loops.addPlaceholder": "+ Agregar escena…",
  "loops.repeatOption": "{name} (repetir)",
  "loops.addSceneGroup": "Escenas",
  "loops.addSnapshotGroup": "Snapshots",
  "loops.snapshotChipHint": "Snapshot — aplica el look capturado completo (valores, FX, master)",
  "loops.holdDisabledHint": "El intervalo lo maneja el BPM global mientras el sync está activo",
  "loops.syncBpm": "Sync al BPM",
  "loops.syncBpmHint":
    "Avanza la lista con la música: cada paso dura la subdivisión elegida al BPM global",
  "loops.syncBpmOff": "El BPM global está apagado — se usa hold/tiempo natural",
  "loops.subdivisionHint": "Cuánto dura cada paso, en tiempo musical",
  "loops.sub.quarter": "Semicorchea (1/16)",
  "loops.sub.half": "Corchea (1/8)",
  "loops.sub.one": "1 beat",
  "loops.sub.two": "2 beats",
  "loops.sub.four": "1 compás",
  "loops.sub.eight": "2 compases",
  "scenes.list.activeLoopPrefix": "Loop: ",
  "scenes.list.activeLoopStop": "Detener",

  "general.title": "General",
  "general.language": "Idioma",
  "general.languageHint": "Se aplica inmediatamente. Guardado por usuario en este equipo.",
  "general.language.en": "English",
  "general.language.es": "Español",
  "general.showFile": "Archivo de show",
  "general.showFileHint":
    "Crear un show nuevo, abrir uno del disco o guardar el actual en una ruta elegida.",

  "sync.title": "Sync",
  "sync.intro":
    "Compartí la config entre tus máquinas via Gist privado de GitHub. Cada usuario usa su propio PAT y su propio gist — nada compartido por defecto. Los Outputs se excluyen del push por defecto (el hardware suele diferir entre Mac y Windows).",
  "sync.section.token": "GitHub PAT",
  "sync.token.saved": "PAT guardado en el keychain.",
  "sync.token.delete": "Borrar",
  "sync.token.deleteConfirm": "¿Borrar el PAT del keychain?",
  "sync.token.placeholder": "ghp_… (scope: gist)",
  "sync.token.save": "Guardar",
  "sync.token.saving": "Guardando…",
  "sync.token.connectedAs": "Conectado como {user}.",
  "sync.token.help":
    "Generá un PAT en github.com/settings/tokens con el scope 'gist'. El token se guarda en el keychain del SO (macOS Keychain / Windows Credential Manager) — nunca toca un archivo.",
  "sync.section.settings": "Configuración",
  "sync.machineLabel": "Etiqueta de esta máquina",
  "sync.machineLabel.placeholder": "ej. mac-julio, windows-estudio",
  "sync.gistId": "Gist id",
  "sync.gistId.placeholder": "dejar vacío para que el primer Push cree uno",
  "sync.includeOutputs":
    "Incluir Outputs en el sync (sólo si las dos máquinas usan el mismo hardware)",
  "sync.saveSettings": "Guardar settings",
  "sync.section.status": "Estado",
  "sync.status.lastPush": "Última subida:",
  "sync.status.lastPull": "Última bajada:",
  "sync.status.lastRemote": "Última versión vista del remoto:",
  "sync.status.never": "nunca",
  "sync.status.from": "desde {machine}",
  "sync.probe": "Comprobar remoto",
  "sync.probing": "Comprobando…",
  "sync.probe.empty": "Remoto: vacío",
  "sync.probe.ahead": "Remoto: {time} · {machine} — adelantado, conviene Pull primero",
  "sync.probe.upToDate": "Remoto: {time} · {machine} — al día",
  "sync.section.actions": "Acciones",
  "sync.action.push": "Push",
  "sync.action.pushing": "Subiendo…",
  "sync.action.forcePush": "Force push",
  "sync.action.forcePushTitle": "Pisar el remoto sin chequear si está adelantado",
  "sync.action.pull": "Pull",
  "sync.action.pulling": "Bajando…",
  "sync.action.pullConfirm":
    "Pull va a sobreescribir el show actual con la versión del gist. ¿Seguir?",
  "sync.backupHint":
    "Antes de cada Pull la app guarda un backup del show actual en sync-backups/. El archivo más nuevo es tu show pre-pull.",
  "sync.toast.pushOk": "Push OK — gist {id}…, remoto {time}",
  "sync.toast.pullOk": "Pull OK — desde {machine}, remoto {time}. Backup en sync-backups/.",

  "remote.title": "Remote (mobile)",
  "remote.intro":
    "Bridge LAN para la app companion en iPhone. Sirve scenes / master / blackout / blind por WiFi local. Sin TLS — usar solo en redes confiables. En Windows el firewall suele pedir permiso la primera vez que arranca: aceptar al menos 'redes privadas'.",
  "remote.status": "Status:",
  "remote.statusRunning": "running",
  "remote.statusStopped": "stopped",
  "remote.start": "Start",
  "remote.stop": "Stop",
  "remote.connectManually": "Conectar manualmente:",
  "remote.clientsConnected": "Clientes conectados:",
  "remote.pairing": "Pairing",
  "remote.pairing.expiresIn": "Caduca en {secs}s. Escaneá el QR desde la app o ingresá el PIN.",
  "remote.pairing.cancel": "Cancelar",
  "remote.pairing.start": "Pair new device",
  "remote.devices": "Devices pareados ({count})",
  "remote.devices.empty": "Ninguno todavía.",
  "remote.devices.pairedAgo": "pareado {ago}",
  "remote.devices.seenAgo": "· visto {ago}",
  "remote.devices.revoke": "Revoke",
  "remote.devices.revokeConfirm": "¿Revocar este dispositivo? Va a perder la conexión.",
  "remote.time.secAgo": "hace {n}s",
  "remote.time.minAgo": "hace {n}m",
  "remote.time.hAgo": "hace {n}h",
  "remote.time.dAgo": "hace {n}d",

  "common.cancel": "Cancelar",
  "common.save": "Guardar",
  "common.delete": "Borrar",
  "common.loading": "Cargando…",
  "common.empty": "Vacío",
  "common.add": "Agregar",
  "common.remove": "Quitar",
  "common.edit": "Editar",
  "common.close": "Cerrar",
  "common.refresh": "Refrescar",
  "common.connect": "Conectar",
  "common.disconnect": "Desconectar",
  "common.connected": "Conectado",
  "common.disconnected": "Desconectado",
  "common.universe": "Universo",
  "common.address": "Dirección",
  "common.channel": "Canal",
  "common.fixture": "Fixture",
  "common.fixtures": "Fixtures",
  "common.mode": "Modo",
  "common.name": "Nombre",
  "common.value": "Valor",
  "common.enabled": "Habilitado",
  "common.disabled": "Deshabilitado",
  "common.optional": "opcional",
  "common.yes": "Sí",
  "common.no": "No",

  "direct.title": "DMX Control — Direct Output (universo 0)",
  "direct.fpsNone": "— FPS",
  "direct.fpsValue": "{fps} FPS",
  "direct.statsMeta": "frames {frames} · late {late} · máx {ms} ms",
  "direct.master": "Master",
  "direct.blackoutOn": "Blackout ON",
  "direct.blackout": "Blackout",
  "direct.clearAll": "Limpiar todo",

  "library.title": "Library ({count})",
  "library.search": "Buscar…",
  "library.reload": "Recargar",
  "library.uploadImage": "Subir imagen",
  "library.changeImage": "Cambiar imagen",
  "library.errorOpenDialog": "No se pudo abrir el diálogo: {err}",
  "library.errorSetImage": "No se pudo cambiar la imagen: {err}",
  "library.modeSummary": "{name} · {count}ch",
  "library.filesPath": "Los archivos JSON viven en {path}.",
  "library.gobosToggle": "Editar gobos / colores",
  "library.gobosCollapse": "Cerrar",
  "library.gobosEmpty":
    "Este fixture no tiene canales con ranges (sin rueda de gobos / colores / macros definida).",
  "library.gobosModeFmt": "{name} ({count}ch)",
  "library.gobosChannelFmt": "{name} · {role}",
  "library.gobosUpload": "Subir",
  "library.gobosReplace": "Reemplazar",
  "library.gobosRangeFmt": "{label} (DMX {from}–{to})",
  "library.gobosErrorOversize": "Imagen muy grande para un thumbnail por range (máx 512 KB).",

  "outputs.title": "Outputs",
  "outputs.kind.mock": "Mock (logging)",
  "outputs.kind.artNet": "Art-Net",
  "outputs.kind.sacn": "sACN (E1.31)",
  "outputs.kind.enttec": "Enttec DMX USB Pro",
  "outputs.kind.openDmx": "Open DMX (OS serial — fallback)",
  "outputs.kind.openDmxFtdi": "Open DMX / ElectroTAS (FTDI directo, recomendado)",
  "outputs.add.mock": "+ Mock",
  "outputs.add.artNet": "+ Art-Net",
  "outputs.artnetScan.button": "🔍 Buscar Art-Net",
  "outputs.artnetScan.hint":
    "Manda un ArtPoll por broadcast y lista todos los nodos Art-Net que respondan en la red",
  "outputs.artnetScan.scanning": "Buscando…",
  "outputs.artnetScan.title": "Nodos Art-Net en la red",
  "outputs.artnetScan.rescan": "Re-escanear",
  "outputs.artnetScan.close": "Cerrar",
  "outputs.artnetScan.inProgress": "Consultando la red — los nodos tienen ~2,5 s para responder…",
  "outputs.artnetScan.none":
    "Ningún nodo respondió. Revisá la interfaz/VLAN, que el nodo esté encendido y que nada bloquee UDP 6454.",
  "outputs.artnetScan.universesOut": "out: {list}",
  "outputs.artnetScan.add": "+ Salida",
  "outputs.artnetScan.addHint":
    "Agrega una salida Art-Net pre-cargada con la IP y los universos de este nodo",
  "outputs.add.sacn": "+ sACN",
  "outputs.add.enttec": "+ Enttec USB",
  "outputs.add.openDmx": "+ Open DMX (OS serial)",
  "outputs.add.openDmxFtdi": "+ Open DMX (FTDI directo)",
  "outputs.rescan": "Re-escanear puertos",
  "outputs.field.target": "Target IP:Puerto",
  "outputs.field.sourceName": "Source name",
  "outputs.field.priority": "Prioridad",
  "outputs.field.serialPort": "Puerto serie",
  "outputs.field.ftdiDevice": "Dispositivo FTDI (por serial)",
  "outputs.field.dtrHigh": "DTR high",
  "outputs.field.rtsHigh": "RTS high",
  "outputs.field.universes": "Universos (separados por coma)",
  "outputs.placeholder.select": "— elegí —",
  "outputs.tag.ftdi": "(FTDI)",
  "outputs.tag.offline": "(offline)",
  "outputs.tag.inUse": "(en uso)",
  "outputs.zadigHint":
    "En Windows, libusb solo ve dispositivos FTDI bindeados a WinUSB. Si tu DMX FTDI no aparece, corré Zadig una vez y elegí el driver WinUSB para esa interface (eso saca el COM port). Si preferís mantener el COM port, cambiá esta salida a 'Serial' y elegí el FTDI desde la lista de puertos.",
  "outputs.remove": "Quitar",
  "outputs.empty": "Sin outputs. Agregá uno arriba.",
  "outputs.dirty": "Cambios sin aplicar",
  "outputs.ok": "OK",
  "outputs.discard": "Descartar",
  "outputs.apply": "Aplicar",

  "patch.title": "Patch ({count} fixtures)",
  "patch.statusClean": "✓ patch limpio",
  "patch.statusIssues": "{conflicts} conflictos · {problems} problemas",
  "patch.qty": "Qty",
  "patch.universe": "U",
  "patch.address": "Addr",
  "patch.labelPlaceholder": "Label (opcional)",
  "patch.add": "Add",
  "patch.modeFmt": "{name} ({count}ch)",
  "patch.col.id": "ID",
  "patch.col.label": "Label",
  "patch.col.fixture": "Fixture",
  "patch.col.mode": "Modo",
  "patch.col.universe": "U",
  "patch.col.address": "Addr",
  "patch.col.range": "Rango",
  "patch.del": "Del",
  "patch.unknownDef": "?? {id}",
  "patch.conflictFmt": "solapan en universo {u} canales {start}…{end}",

  "buttons.title": "Botones omnipresentes",
  "buttons.subhead": "Fades en milisegundos · in/out independientes",
  "buttons.section.master": "Master",
  "buttons.section.blackout": "Blackout",
  "buttons.section.blind": "Blind (halógeno)",
  "buttons.masterIntro":
    "Canales que el master fader escala. Sin fixtures asignados = automático: cada fixture patcheado escala su intensidad (o RGB si no tiene dimmer); pan/tilt/colour-wheel/etc. quedan intactos. Asigná fixtures abajo si querés override del set de canales por fixture (ej. incluir W/A en pares RGBWAP).",
  "buttons.assignedMaster": "Fixtures asignados al Master ({count})",
  "buttons.assignedMasterAuto": "Fixtures asignados al Master (auto · {count})",
  "buttons.emptyMaster": "Patcheá fixtures primero para poder asignarlos al master.",
  "buttons.masterHintAuto":
    "Auto: se escala intensidad (o R/G/B si no hay dimmer). Otros canales no se tocan.",
  "buttons.masterHintCustom": "Sólo estos canales son escalados por el master fader.",
  "buttons.blackoutIntro":
    "Apaga (con cross-fade) los canales que elijas de cada fixture. Sin fixtures asignados = modo automático: todos los fixtures patcheados apagan intensidad (o RGB si no tienen dimmer) + strobe; pan/tilt/zoom no se tocan para que los cabezales no salten al piso. Si querés algo específico (matar solo intensity, o también un canal de macro custom), agregá los fixtures abajo y tildá los canales que tienen que ir a 0.",
  "buttons.blindIntro":
    "Hold-to-flash con cross-fade contra el estado actual de cada luz. Mantené el botón: los fixtures asignados encienden con un fade-in rápido en color halógeno; al soltar, fade-out lento que vuelve al color que tenía antes (si era verde a 50%, vuelve al verde). Por defecto el blind escribe warm-white sobre intensity + RGB. Si querés que se active otro canal específico (strobe, shutter, función custom), tildalo en la fila del fixture y ese canal va a slamearse a 255 en lugar del halógeno default.",
  "buttons.fadeIn": "Fade in (ms)",
  "buttons.fadeOut": "Fade out (ms)",
  "buttons.assignedBlackout": "Fixtures asignados al Blackout ({count})",
  "buttons.assignedBlackoutAuto": "Fixtures asignados al Blackout (auto · {count})",
  "buttons.assignedBlind": "Fixtures asignados al Blind ({count})",
  "buttons.emptyBlackout": "Patcheá fixtures primero para poder asignarlos al blackout.",
  "buttons.emptyBlind": "Patcheá fixtures primero para poder asignarlos al blind.",
  "buttons.blackoutHintAuto": "Auto: intensity (o RGB si no hay) + strobe → 0. Pan/tilt intactos.",
  "buttons.blackoutHintCustom": "Solo estos canales se llevan a 0.",
  "buttons.blindHintAuto": "Halógeno default (warm-white sobre intensity + RGB).",
  "buttons.blindHintCustom": "Solo estos canales se mandan a 255 (sin halógeno).",

  "midi.title": "MIDI",
  "midi.intro":
    "Conectá una superficie de control (ej. Launchpad MK2). El primer dispositivo compatible se conecta al arranque; podés sobrescribirlo acá.",
  "midi.devices": "Dispositivos",
  "midi.refresh": "Refrescar",
  "midi.connect": "Conectar",
  "midi.disconnect": "Desconectar",
  "midi.connected": "Conectado a {name}",
  "midi.notConnected": "Sin conexión.",
  "midi.lastEvent": "Último evento: {event}",
  "midi.colorTest": "Test de color",
  "midi.empty": "No se encontraron dispositivos MIDI.",
  "midi.refreshHint":
    "Conectá un controlador MIDI (Launchpad MK2, etc.) para usarlo como surface. Cuando conectás un Launchpad, el surface controller toma la fila inferior (8 chasers, uno por pad) y los scene buttons del lateral derecho (rojo = blackout, blanco = blind momentáneo).",
  "midi.detectedDevices": "Dispositivos detectados ({count})",
  "midi.errListing": "No se pudo listar dispositivos MIDI: {err}",
  "midi.errConnect": "No se pudo conectar a {name}: {err}",
  "midi.errDisconnect": "No se pudo desconectar: {err}",
  "midi.errTest": "Falló el test de pads: {err}",
  "midi.in": "in",
  "midi.out": "out",
  "midi.bothIO": "in + out",
  "midi.onlyIn": "sólo in",
  "midi.surfaceActive": "surface activo",
  "midi.testPads": "Test pads (Launchpad MK2)",
  "midi.statusSection": "Estado",
  "midi.connectedToFmt": "Conectado a {name} ({io}){surface}",

  "streamdeck.title": "Stream Deck",
  "streamdeck.intro":
    "Conectá un Elgato Stream Deck y el surface controller toma las teclas: chasers en fila 1, movimientos en fila 2, escenas en fila 3 más blind (momentáneo) y blackout (toggle). Layout fijo por ahora — primeros 5 chasers/movimientos y primeras 3 escenas.",
  "streamdeck.refresh": "Refrescar",
  "streamdeck.connect": "Conectar",
  "streamdeck.disconnect": "Desconectar",
  "streamdeck.empty": "No se encontraron Stream Decks.",
  "streamdeck.detectedDevices": "Dispositivos detectados ({count})",
  "streamdeck.deviceMeta": "{keys} teclas · {serial}",
  "streamdeck.statusSection": "Estado",
  "streamdeck.connectedToFmt": "Conectado a {kind} ({keys} teclas) · surface activo",
  "streamdeck.errListing": "No se pudieron listar los Stream Decks: {err}",
  "streamdeck.errConnect": "No se pudo conectar a {name}: {err}",
  "streamdeck.errDisconnect": "No se pudo desconectar: {err}",

  "ai.title": "IA",
  "ai.intro":
    "Provider, modelo y API key para generación de escenas con LLM. Las keys quedan en el dir de config del SO, fuera del show file.",
  "ai.provider": "Provider",
  "ai.provider.anthropic": "Anthropic (Claude)",
  "ai.provider.openai": "OpenAI",
  "ai.apiKey": "API key",
  "ai.apiKey.show": "Ver",
  "ai.apiKey.hide": "Ocultar",
  "ai.model": "Modelo",
  "ai.test": "Probar conexión",
  "ai.testing": "Probando…",
  "ai.testOk": "Conexión OK.",
  "ai.testFail": "Conexión falló: {err}",
  "ai.save": "Guardar",
  "ai.saved": "Guardado.",
  "ai.refreshModels": "Refrescar modelos",
  "ai.subhead": "Generación de escenas con LLM (Anthropic / OpenAI) — POC",
  "ai.warning":
    "Las API keys se guardan en texto plano en el directorio de configuración del SO (fuera del archivo del show). En producción deberían moverse al keychain del SO.",
  "ai.providerActive": "Provider activo",
  "ai.provider.none": "Desactivado",
  "ai.provider.anthropicLong": "Anthropic (Claude)",
  "ai.provider.openaiLong": "OpenAI (GPT)",
  "ai.section.anthropic": "Anthropic",
  "ai.section.openai": "OpenAI",
  "ai.keyHint.anthropic": "Conseguí una API key en",
  "ai.keyHint.openai": "Conseguí una API key en",
  "ai.toggleKey.show": "Mostrar key",
  "ai.toggleKey.hide": "Ocultar key",
  "ai.testDisabledHint": "Elegí un provider y completá la API key primero",
  "ai.testActiveHint": "Hacer un request mínimo para verificar la API key + modelo",
  "ai.savedToast": "Configuración guardada.",
  "ai.errPrefix": "Error: {err}",

  "scenes.title": "Escenas",
  "scenes.metaSummary": "Multi-step + FX capture · {count} escena{plural}",
  "scenes.aiTrigger": "✨ Generar con IA",
  "scenes.aiTriggerHint": "Generar una escena nueva con IA a partir de un prompt",
  "scenes.errCreate": "No se pudo crear la escena: {err}",
  "scenes.list.new": "+ Nueva escena",
  "scenes.list.empty":
    'Sin escenas todavía. Armá un look en Stage y tocá "Nueva escena" para grabarlo.',
  "scenes.list.lpHint": "LP fila 3, pad {pad}",
  "scenes.list.recallHint": "Recall (▶ GO)",
  "scenes.list.stepCount": "{count} step{plural}",
  "scenes.list.activePrefix": "▶ {name}",
  "scenes.list.activeStep": " · paso {step}",
  "scenes.list.release": "Liberar",
  "scenes.editor.empty": "Elegí una escena de la izquierda — o creá una nueva.",
  "scenes.editor.recallHint": "Recall esta escena (▶ GO)",
  "scenes.editor.go": "▶ GO",
  "scenes.editor.cycleTotal": "Ciclo total: {seconds}s",
  "scenes.editor.aiIterate": "✨ Mejorar con IA",
  "scenes.editor.aiIterateHint":
    "Iterar esta escena con IA (refinar valores, fades, agregar/quitar steps)",
  "scenes.editor.delete": "Eliminar",
  "scenes.editor.deleteConfirm": '¿Eliminar "{name}"?',
  "scenes.editor.fxHint":
    "Cada step graba el chaser y movement activos en ese instante. Al frenar la escena se restaura lo que estaba corriendo antes del recall. Editá el FX state por step abajo.",
  "scenes.editor.stepsHeading": "Steps ({count})",
  "scenes.editor.stepsEmpty": "La escena no tiene steps. Agregá uno desde el bloque de abajo.",
  "scenes.editor.addStepHeading": "Agregar step desde el estado actual",
  "scenes.editor.fadeIn": "Fade in (ms)",
  "scenes.editor.hold": "Hold (ms)",
  "scenes.editor.touchedOnly": "Solo touched ({count})",
  "scenes.editor.addStep": "+ Add step",
  "scenes.editor.addDisabledHint": "Tocá fixtures en Stage primero",
  "scenes.editor.addEnabledHint": "Capturar el estado actual como nuevo step",
  "scenes.editor.loopHint":
    "Los pasos se reproducen en orden y vuelven al primero al final, formando un loop. El siguiente paso arranca cuando termina el hold del actual.",
  "scenes.editor.fixturesHint":
    '{writes} writes totales sobre {fixtures} fixtures patcheados. Tip: si hace falta, hacé Clear del programmer y usá "Solo touched" para iterar steps sin pisarte de más.',
  "scenes.editor.clearProg": "Clear programmer",
  "scenes.programmer.label": "PROG · {count} fixture{plural} tocado{plural}",
  "scenes.programmer.clear": "Clear",
  "snapshots.heading": "Snapshots",
  "snapshots.capture": "● Grabar snapshot",
  "snapshots.captureHint":
    "Congela todo el look actual: valores DMX, master, chaser, movement, escena/loop activos, blackout y BPM",
  "snapshots.empty": "Sin snapshots todavía. Grabá uno para congelar el look actual.",
  "snapshots.activateHint":
    "Aplicar este snapshot. El estado actual se recuerda — al desactivar vuelve tal cual estaba.",
  "snapshots.deactivate": "Desactivar",
  "snapshots.deactivateHint": "Volver al estado previo a la activación del snapshot",
  "snapshots.recaptureHint": "Sobrescribir este snapshot con el estado actual del rig",
  "snapshots.deleteConfirm": '¿Eliminar el snapshot "{name}"?',
  "snapshots.deleteHint": "Eliminar snapshot",
  "snapshots.activeBadge": "ACTIVO",
  "scenes.step.fade": "Fade",
  "scenes.step.hold": "Hold",
  "scenes.step.ms": "ms",
  "scenes.step.metaFmt": "{fixtures}f · {channels}ch",
  "scenes.step.placeholder": "Step {n}",
  "scenes.step.update": "⟳",
  "scenes.step.updateAllHint":
    "Re-grabar este step con el estado actual del rig (todos sus fixtures)",
  "scenes.step.updateTouched": "⟳T",
  "scenes.step.updateTouchedHint": "Re-grabar solo los fixtures touched (resto queda como está)",
  "scenes.step.removeHint": "Eliminar este step",
  "scenes.step.removeOnlyHint": "No se puede eliminar el único step",
  "scenes.step.removeConfirm": "¿Eliminar el step {n}?",
  "scenes.fx.chaser": "Chaser",
  "scenes.fx.movement": "Movement",
  "scenes.fx.inherit": "No tocar",
  "scenes.fx.disable": "Apagar",
  "scenes.fx.enable": "Encender:",
  "scenes.fx.noOptions": "— sin opciones —",

  "chaser.title": "Ambient Chaser ({count})",
  "chaser.addExample": "+ Chasers de ejemplo",
  "chaser.addNew": "+ Nuevo chaser",
  "chaser.empty":
    "Sin chasers. Creá uno y asignale fixtures, o probá + Chasers de ejemplo para empezar con presets ya configurados.",
  "chaser.exampleNoticeNoFixtures":
    "Se agregaron {count} chasers de ejemplo. Patcheá fixtures y agregalos a sus slots para verlos en acción.",
  "chaser.exampleNoticeWithFixtures":
    "Se agregaron {count} chasers de ejemplo con tus {fixtures} fixture(s) ya asignados.",
  "chaser.toggle.on": "ON",
  "chaser.toggle.off": "OFF",
  "chaser.toggle.disable": "Apagar chaser",
  "chaser.toggle.enable": "Encender chaser",
  "chaser.lpHint": "Launchpad pad {pad} (fila inferior)",
  "chaser.summary": "{bpm} BPM · {slots} slots",
  "chaser.summary.fade": " · fade",
  "chaser.hide": "Ocultar",
  "chaser.edit": "Editar",
  "chaser.del": "Del",
  "chaser.tab.pattern": "Pattern & Color",
  "chaser.tab.timing": "Timing & Fade",
  "chaser.tab.slots": "Slots ({count})",
  "chaser.section.pattern": "Pattern",
  "chaser.section.colorMode": "Modo de color",
  "chaser.section.tempo": "Tempo",
  "chaser.section.levels": "Niveles",
  "chaser.section.fade": "Fade entre steps",
  "chaser.bpm": "BPM",
  "chaser.subdivision": "Subdivisión",
  "chaser.master": "Master",
  "chaser.background": "Background",
  "chaser.fadeEnabled": "Habilitado",
  "chaser.fadeAmount": "Amount (% del step)",
  "chaser.fadeCurve": "Curva",
  "chaser.fadeHint":
    "0% = snap (off→on instantáneo). 90% = casi todo el step crossfading. La curva define el shape de la transición.",
  "chaser.preview.empty": "Sin slots — agregá fixtures para ver el efecto.",
  "chaser.slots.empty": "Sin slots — agregá fixtures abajo.",
  "chaser.slots.intLabel": "Int",
  "chaser.slots.colorLabel": "Color",
  "chaser.slots.add": "+ Agregar slot",
  "chaser.slots.fixtureFmt": "{label} (U{universe}/{address})",
  "chaser.cadence": "Cadence",
  "chaser.cadenceN": "N steps",
  "chaser.rotation": "Rotación",
  "chaser.removeColor": "Quitar color",
  "chaser.addColor": "+ Agregar color",
  "chaser.rainbowSpeed": "Velocidad (deg/step)",
  "chaser.rainbowSpread": "Spread (largo del arcoiris)",
  "chaser.fadeCurveLabel.linear": "Linear",
  "chaser.fadeCurveLabel.easeInOut": "Ease in/out",
  "chaser.fadeCurveLabel.easeIn": "Ease in",
  "chaser.fadeCurveLabel.easeOut": "Ease out",
  "chaser.fadeCurveLabel.exponential": "Exponential",
  "chaser.fadeCurveLabel.logarithmic": "Logarithmic",
  "chaser.subdivisionLabel.16": "1/16",
  "chaser.subdivisionLabel.8": "1/8",
  "chaser.subdivisionLabel.4": "1/4",
  "chaser.subdivisionLabel.2": "1/2",
  "chaser.subdivisionLabel.1": "1/1",
  "chaser.patternLabel.allTogether": "Todos juntos",
  "chaser.patternLabel.alternate": "Alternar",
  "chaser.patternLabel.chase": "Chase →",
  "chaser.patternLabel.chaseReverse": "Chase ←",
  "chaser.patternLabel.pingPong": "Ping-pong",
  "chaser.patternLabel.wave": "Wave →",
  "chaser.patternLabel.waveReverse": "Wave ←",
  "chaser.patternLabel.build": "Build",
  "chaser.patternLabel.buildReverse": "Build reverse",
  "chaser.patternLabel.centerOut": "Centro hacia afuera",
  "chaser.patternLabel.symmetric": "Simétrico",
  "chaser.patternLabel.random": "Random",
  "chaser.patternLabel.outsideIn": "Hacia adentro",
  "chaser.patternLabel.invertedChase": "Chase invertido (sombra)",
  "chaser.patternLabel.groupsOfTwo": "Grupos de dos",
  "chaser.patternLabel.groupsOfThree": "Grupos de tres",
  "chaser.patternLabel.halfSwap": "Intercambio de mitades",
  "chaser.patternLabel.edges": "Sólo bordes",
  "chaser.patternLabel.pulseOut": "Pulso hacia afuera (sonar)",
  "chaser.patternLabel.pulseIn": "Pulso hacia adentro",
  "chaser.patternLabel.accordion": "Acordeón (respiración)",
  "chaser.patternLabel.bowtie": "Moño (build desde bordes)",
  "chaser.patternLabel.dualChase": "Chase doble (heads paralelos)",
  "chaser.patternLabel.symmetricBounce": "Vaivén simétrico",
  "chaser.cadenceLabel.everyStep": "Cada step (A B A B…)",
  "chaser.cadenceLabel.everyN": "Cada N steps (AAAA BBBB)",
  "chaser.cadenceLabel.perSlot": "Por slot (mitad A, mitad B)",
  "chaser.cadenceLabel.alternateSlots": "Alternar slots (cebra)",
  "chaser.cadenceLabel.chasePerColor": "Un chase A, próximo chase B",
  "chaser.rotationLabel.perStep": "Por step",
  "chaser.rotationLabel.perCycle": "Por ciclo (cada chase)",
  "chaser.rotationLabel.perSlot": "Por slot (estático)",
  "chaser.colorModeLabel.disabled": "Sin color (sólo intensidad)",
  "chaser.colorModeLabel.single": "Color único",
  "chaser.colorModeLabel.twoColor": "Cadencia de dos colores",
  "chaser.colorModeLabel.palette": "Paleta",
  "chaser.colorModeLabel.rainbow": "Arcoiris",

  "movement.title": "Movement Generators",
  "movement.addNew": "+ Agregar movement",
  "movement.empty":
    "Sin movements. Creá uno para empezar — el primero queda mapeado al pad 21 del Launchpad (fila 2). Los siguientes ocupan los 7 pads a la derecha.",
  "movement.delete": "Eliminar",
  "movement.deleteConfirm": '¿Eliminar "{name}"?',
  "movement.lpHint": "Launchpad pad {pad} (fila 2)",
  "movement.legend.fixtures": "Fixtures ({count})",
  "movement.legend.shape": "Shape",
  "movement.legend.canon": "Canon",
  "movement.legend.timing": "Timing",
  "movement.legend.transform": "Transform",
  "movement.fixtures.empty": "Sin fixtures asignados.",
  "movement.fixtures.invertPan": "Inv P",
  "movement.fixtures.invertTilt": "Inv T",
  "movement.fixtures.add": "+ Agregar fixture…",
  "movement.fixtures.fixtureFmt": "{label} (U{universe}/{address})",
  "movement.shape.sides": "Lados",
  "movement.shape.points": "Puntas",
  "movement.shape.innerRatio": "Inner ratio",
  "movement.timing.bpm": "BPM",
  "movement.timing.loopLength": "Largo del loop",
  "movement.transform.sizeX": "Size X",
  "movement.transform.sizeY": "Size Y",
  "movement.transform.centerX": "Center X",
  "movement.transform.centerY": "Center Y",
  "movement.transform.rotation": "Rotación",
  "movement.transform.reset": "Reset transform",
  "movement.previewHint":
    "Sub-fase A: solo Circle. Más shapes (Polygon, Star, Lissajous, Sine combos…) en sub-fases C y D.",
  "movement.previewBadgeOff": "OFF",
  "movement.previewMeta": "{count} fixtures · phase {phase}%",
  "movement.previewPaused": "{count} fixtures · pausado",
  "movement.wave.pan": "Pan",
  "movement.wave.tilt": "Tilt",
  "movement.wave.waveform": "Waveform",
  "movement.wave.frequency": "Frecuencia",
  "movement.wave.phaseShift": "Phase shift",
  "movement.wave.amplitude": "Amplitud",
  "movement.wave.offset": "Offset",
  "movement.preset.label": "Presets:",
  "movement.preset.loadHint": "Cargar preset {name}",
  "movement.shapeLabel.circle": "Círculo",
  "movement.shapeLabel.polygon": "Polígono",
  "movement.shapeLabel.star": "Estrella",
  "movement.shapeLabel.figureEight": "Figure 8",
  "movement.shapeLabel.lineH": "Línea ⇄",
  "movement.shapeLabel.lineV": "Línea ⇅",
  "movement.shapeLabel.sineCombo": "Sine combo",
  "movement.spreadLabel.none": "Sin spread (todos en fase)",
  "movement.spreadLabel.even": "Parejo (canon)",
  "movement.spreadLabel.symmetric": "Simétrico",
  "movement.spreadLabel.pairs": "De a pares",
  "movement.spreadLabel.manual": "Manual",
  "movement.directionLabel.forward": "Forward →",
  "movement.directionLabel.reverse": "Reverse ←",
  "movement.directionLabel.pingPong": "Ping-pong ↔",
  "movement.subdivLabel.16": "1/16 (muy rápido)",
  "movement.subdivLabel.8": "1/8",
  "movement.subdivLabel.1beat": "1 beat",
  "movement.subdivLabel.2beats": "2 beats",
  "movement.subdivLabel.4beats": "4 beats",
  "movement.subdivLabel.8beats": "8 beats",
  "movement.waveformLabel.sine": "Sine",
  "movement.waveformLabel.cosine": "Cosine",
  "movement.waveformLabel.triangle": "Triangle",
  "movement.waveformLabel.square": "Square",
  "movement.waveformLabel.sawtooth": "Sawtooth",
  "movement.waveformLabel.rampUp": "Ramp ↑",
  "movement.waveformLabel.rampDown": "Ramp ↓",
  "movement.presetLabel.circle": "Círculo",
  "movement.presetLabel.figureEight": "Figure 8 (1:2)",
  "movement.presetLabel.lissajous32": "Lissajous 3:2",
  "movement.presetLabel.lissajous54": "Lissajous 5:4",
  "movement.presetLabel.wavePan": "Wave (sólo pan)",

  "stage.title": "Stage",
  "stage.meta": "{count} fixtures · grid {grid}px",
  "stage.metaSelected": " · {count} seleccionados",
  "stage.empty": "Sin fixtures.",
  "stage.sidebar.placeholder":
    "Seleccioná un fixture o un tipo para abrir sus encoders. ⌘/Ctrl-click suma o quita de la selección.",
  "stage.fixture.untouchAria": "Quitar de touched ({count} canales)",
  "stage.fixture.untouchTitle": "Click para quitar de touched · canales: {labels}",
  "stage.type.selectHint": "Seleccionar las {count} unidades (⌘/Ctrl-click para sumar)",
  "stage.editor.unavailable": "Definición no disponible.",
  "stage.editor.multiHeading": "{fixtures} fixtures · {types} tipos",
  "stage.editor.multiBadge": "×{count}",
  "stage.editor.mixedHint":
    "Mostrando solo los controles comunes a todas las unidades seleccionadas.",
  "stage.editor.activeFx": "Efectos activos",
  "stage.editor.fxKindChaser": "Chaser",
  "stage.editor.fxKindMove": "Move",
  "stage.editor.fxTooltipChaser": "Chaser: {name} · afecta {touches}/{total}",
  "stage.editor.fxTooltipMovement": "Movement: {name} · afecta {touches}/{total}",
  "stage.editor.center": "Centro",
  "stage.editor.home": "Home",
  "stage.editor.section.intStrobe": "Intensidad y estrobo",
  "stage.editor.section.color": "Color",
  "stage.editor.section.extras": "Extras",
  "stage.fxbar.scenes": "Escenas",
  "stage.fxbar.snapshots": "Snapshots",
  "stage.fxbar.movements": "Movements",
  "stage.fxbar.chasers": "Chasers",
  "stage.fxbar.releaseHint": "Liberar — la rig queda en su estado actual",
  "stage.fxbar.idle": "— sin escena activa",
  "stage.fxbar.openScenes": "Abrir panel de escenas",
  "stage.fxbar.closeScenes": "Cerrar panel de escenas",
  "stage.fxbar.scenesBtnClose": "Cerrar",
  "stage.fxbar.scenesBtnOpen": "Escenas…",
  "stage.fxbar.morePill": "+{n} más",
  "stage.fxbar.scenePillTitle": "▶ {name} ({count} step{plural})",
  "stage.fxbar.disableMovement": "Apagar {name}",
  "stage.fxbar.enableMovement": "Encender {name} ({slots} slots)",
  "stage.fxbar.disableChaser": "Apagar {name}",
  "stage.fxbar.enableChaser": "Encender {name} ({slots} slots)",
  "stage.fxbar.stepName": "{name}",
  "stage.fxbar.stepFmt": "paso {step}/{total}",
  "stage.sqp.aria": "Panel de escenas",
  "stage.sqp.title": "Escenas",
  "stage.sqp.metaFmt": "{scenes} · {touched} touched",
  "stage.sqp.list": "Lista",
  "stage.sqp.newScene": "+ Nueva",
  "stage.sqp.empty": "Sin escenas. Armá un look y tocá + Nueva para grabarlo.",
  "stage.sqp.recallTitle": "Recall ({count} step{plural})",
  "stage.sqp.editSceneTitle": "Editar steps de esta escena",
  "stage.sqp.sceneStepFmt": "{count} step{plural}",
  "stage.sqp.sceneLive": " · live",
  "stage.sqp.releaseActive": "Liberar escena activa",
  "stage.sqp.stepsHeading": "Steps de {name}",
  "stage.sqp.tagLive": "EN VIVO",
  "stage.sqp.tagStopped": "FRENADA",
  "stage.sqp.followActive": "Seguir activa",
  "stage.sqp.followActiveHint": "Volver a seguir la escena activa",
  "stage.sqp.stepPlaceholder": "Step {n}",
  "stage.sqp.fadeTitle": "Fade in (ms)",
  "stage.sqp.holdTitle": "Hold (ms)",
  "stage.sqp.overwriteAll": "Sobreescribir este step con el estado actual del rig",
  "stage.sqp.overwriteTouched": "Sobreescribir solo los fixtures touched",
  "stage.sqp.removeStep": "Eliminar step",
  "stage.sqp.removeOnlyHint": "No se puede eliminar el único step",
  "stage.sqp.recordHeadingNew": "Grabar nueva escena",
  "stage.sqp.recordHeadingAdd": 'Agregar step a "{name}"',
  "stage.sqp.fade": "Fade",
  "stage.sqp.hold": "Hold",
  "stage.sqp.ms": "ms",
  "stage.sqp.touchedOnly": "Solo touched ({count})",
  "stage.sqp.addStep": "+ Add step",
  "stage.sqp.recordNew": "● Record",
  "stage.sqp.touchedHelperOn": "Ocultar halos de fixtures touched en la canvas",
  "stage.sqp.touchedHelperOff":
    "Mostrar halos de fixtures touched en la canvas mientras este panel esté abierto",
  "stage.sqp.touchedToggle": "👁 Touched",
  "stage.sqp.locateHint": "Hacer un pulse en los fixtures touched para encontrarlos en la canvas",
  "stage.sqp.locate": "📍 Localizar",
  "stage.sqp.clearProg": "Clear PROG",
  "stage.sqp.errRecord": "No se pudo grabar: {err}",
  "stage.sqp.errAddStep": "No se pudo agregar el step: {err}",
  "stage.sqp.errCreate": "No se pudo crear: {err}",
  "stage.sqp.errNoTouched": "No hay fixtures tocados; movés un slider para marcarlos.",
  "stage.menu.fixtureSingular": "Fixture",
  "stage.menu.fixturePlural": "{count} fixtures",
  "stage.menu.centerPanTilt": "Centrar Pan/Tilt",
  "stage.menu.park": "Park (defaults)",
  "stage.menu.fullIntensity": "Intensidad al máximo",
  "stage.menu.blackoutFixture": "Blackout (intensity 0)",
  "stage.menu.rename": "Renombrar…",
  "stage.menu.duplicate": "Duplicar",
  "stage.menu.untouch": "Untouch",
  "stage.menu.remove": "Eliminar",
  "stage.confirm.removeOne": '¿Eliminar "{name}"?',
  "stage.confirm.removeMany": "¿Eliminar {count} fixtures?",

  "aiGen.aria": "Generar escena con IA",
  "aiGen.title": "Generar escena con IA",
  "aiGen.titleIterating": "Mejorando: {name}",
  "aiGen.errPromptEmpty": "Escribí un prompt — ej: 'cálido amber con fade lento'.",
  "aiGen.errScopeEmpty": "Tildá al menos un fixture en la lista o cambiá el scope a 'todos'.",
  "aiGen.errRefineEmpty": "Escribí qué querés cambiar (ej: 'el step 2 más rápido').",
  "aiGen.field.prompt": "Prompt",
  "aiGen.field.promptPlaceholder":
    "ej: 'pulsos cálidos amber sincronizados, fade 600ms, hold 400ms' · 'recorrido frío azul lento de izquierda a derecha' · 'punk rojo strobe alterno'",
  "aiGen.field.stepCount": "Cantidad de steps",
  "aiGen.field.scope": "Scope",
  "aiGen.scope.all": "Todos los fixtures ({count})",
  "aiGen.scope.selected": "Solo seleccionados",
  "aiGen.scope.fixturesHint":
    "Tildá los fixtures que querés que la IA mueva. Los demás quedan como están.",
  "aiGen.disabledHint": "Agregá fixtures al patch primero",
  "aiGen.activeHint": "Generar escena (puede tardar 5-15 segundos)",
  "aiGen.generating": "Generando…",
  "aiGen.generate": "✨ Generar",
  "aiGen.preview.stepCount": "{count} step{plural}",
  "aiGen.preview.stepPlaceholder": "Step {n}",
  "aiGen.preview.stepMeta": "fade {fade}ms · hold {hold}ms · {count} fixture{plural}",
  "aiGen.refine": "Refinar",
  "aiGen.refinePlaceholder":
    "ej: 'el step 2 más rápido' · 'menos azul, más amber' · 'agregá strobe en el último'",
  "aiGen.refineHint": "Aplicar el cambio sobre el draft actual sin perder lo bueno",
  "aiGen.refining": "Pensando…",
  "aiGen.refineLabel": "↻ Refinar",
  "aiGen.resetFromScratch": "Empezar de cero",
  "aiGen.replaceOriginal": "Reemplazar original",
  "aiGen.replaceOriginalHint": 'Sobrescribir los steps de "{name}" con este draft',
  "aiGen.replacing": "Reemplazando…",
  "aiGen.applyNew": "Aplicar como escena nueva",
  "aiGen.applying": "Aplicando…",

  "p3d.types.title": "Por tipo de fixture ({count})",
  "p3d.types.intro":
    "Brillo, ángulo de haz y prisma se configuran por modelo de fixture — todas las instancias del mismo tipo comparten estos valores.",
  "p3d.types.metaPrefix": "{count}× · brillo {brightness}",
  "p3d.types.brightness": "Brillo",
  "p3d.types.brightnessHint":
    "Multiplica la intensidad renderizada (haz + spotlight + halo). Default 1.0.",
  "p3d.types.beamOverride": "Override de ángulo de haz",
  "p3d.types.beamMin": "Mín °",
  "p3d.types.beamMax": "Máx °",
  "p3d.types.beamMinHint": "Half-angle del beam con zoom cerrado",
  "p3d.types.beamMaxHint": "Half-angle del beam con zoom abierto",
  "p3d.types.prismOverride": "Override de prisma",
  "p3d.types.prismThreshold": "Threshold",
  "p3d.types.prismFacets": "Facetas",
  "p3d.types.prismSplay": "Splay °",
  "p3d.types.prismThresholdHint": "DMX value a partir del cual el prisma se activa",
  "p3d.types.prismFacetsHint": "Cantidad total de haces (incluyendo el central)",
  "p3d.types.prismSplayHint": "Apertura entre haces",
  "p3d.types.reset": "Quitar overrides (volver a defaults)",
};

export type LanguageCode = "en" | "es";

export const LANGUAGES: { code: LanguageCode; nativeName: string }[] = [
  { code: "en", nativeName: "English" },
  { code: "es", nativeName: "Español" },
];

export const DICTIONARIES: Record<LanguageCode, Translation> = { en, es };
