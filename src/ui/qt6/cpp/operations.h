// Driving a file operation from the UI.
//
// Three steps, and the middle one is the point: prepare produces a plan, the
// user is shown what it will do, and only then does anything run
// (docs/UI_UX_SPEC.md 10). Nothing here blocks: the work happens on a Rust
// worker thread and the window polls it on its existing tick.
#pragma once

#include "bridge.h"

#include <QString>
#include <QWidget>

namespace ops {

// Wait for a plan the model is building on a worker thread.
//
// Returns true when a plan is ready. Shows a cancellable "counting" dialog if
// it takes long enough to notice - a quick plan never flashes one. The wait
// runs a nested event loop, so the window keeps painting instead of
// beachballing; the dialog is modal, so no second operation can start
// underneath it.
bool awaitPlan(JtfApp *app, QWidget *parent);

enum Kind { Copy = 0, Move = 1, Trash = 2, Delete = 3 };

// Prepares, asks whatever needs asking, and starts. Returns false if the user
// declined or there was nothing to do; in the latter case `message` carries a
// localized explanation to show.
bool confirmAndStart(JtfApp *app, QWidget *parent, int pane, Kind kind, QString *message);

// Rename and new folder ask for a name first.
bool renameSelection(JtfApp *app, QWidget *parent, int pane, QString *message);
bool createFolder(JtfApp *app, QWidget *parent, int pane, QString *message);

// Asks for a name and creates an empty file.
bool createFile(JtfApp *app, QWidget *parent, int pane, QString *message);

// Localized one-line summary of the finished operation, cleared as it is read.
QString takeResult(JtfApp *app);

// Asks how to resolve conflicts. Returns the policy, or -1 if the user backed
// out. Shared with the drop path, which prepares its own plan.
int askConflictPolicy(JtfApp *app, QWidget *parent, int conflicts);

} // namespace ops
