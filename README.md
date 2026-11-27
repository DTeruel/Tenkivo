# Tenkivo

A local desktop prototype for organizing game design references on an infinite canvas. Built with Tauri 2, Rust, Svelte and TypeScript.

## Release 0.14.0

The workspace is now **Tenkivo**. Existing `.gdstudio` projects keep the same format.

## Release 0.13.0

Hold **Alt** in edit mode to reveal ports on canvas objects. Drag between ports to connect objects with an arrow. Select an arrow and press **Delete** to remove it. Older diagram objects open as frames containing their nodes and connections.

## Release 0.12.1

Drawing mode groups the strokes in each draft into one canvas object. Use **Done** to accept and exit, **Enter** to accept and start a fresh drawing, or **Esc** to discard the current draft and exit.

## Release 0.12.0

Choose **Drawing** from the canvas add menu. Pick a color and brush size, draw on the canvas, then choose **Done** to return to selection. Each stroke is an object that can be moved, resized, rotated or deleted. Drawings are included in PDF exports.

Choose **Diagram** to add a diagram with a starter node. Open its editor to move nodes, enter titles and descriptions, and connect side ports. Choose an arrow to move either end or delete it. Open a node’s subdiagram to work on a nested level.

## Release 0.11.0

Right-click a page and choose **View page history** to see saved versions and their authors and dates. Preview the contents before restoring. Restoration keeps later versions and records a new version. Save to persist the change. Projects created before page history starts show an initial snapshot with unknown authorship and date.

## Release 0.10.2

Choose **Copy to project** in the inspector to copy a linked file or folder into the project and update the selected card. Save the project to keep the new reference.

## Release 0.10.1

Open folder now reveals the exact attachment in Windows Explorer.

## Release 0.10.0

Manage names and icons under **Project settings → Users**. Select a user each time you enter edit mode. The last user selected on this device appears first. Existing projects can register their first user from the selector. Save to keep user registrations with the project.

## Release 0.9.3

Drop a local video onto the canvas and choose to copy or link it. Use the embedded player in read or edit mode. Playback depends on the video codec supported by the system webview. Attachment location and relinking are now at the end of the inspector.

## Release 0.9.2

Play audio directly on the canvas. Drop a recording and choose **Copy to project** or **Link original**. Audio cards include playback, seeking and volume controls in read and edit mode. Playback support depends on the codec available in the system webview.

## Release 0.9.1

Use File → Close to return to the welcome screen. Canvas Snapping is available in Settings both before and after opening a project. Hold Shift while dragging a corner to preserve proportions. Use the plus beside Departments in Project settings to add a department.

## Release 0.9.0

Start from the welcome screen to create a project, open a file or revisit a recent project. Recent entries show their saved location and availability. Remove a missing entry to forget its shortcut. In Settings, choose **On startup → Reopen last project** to load the previous file directly. New desktop projects ask for their file location when created. Settings, Project settings, Keyboard shortcuts and Editor logs close from their top-right corner.

## Release 0.8.2

Toggle **Show title** in the image inspector or context menu. The setting is saved with the image and applies to PDF exports. After saving a PDF on Windows, click **PDF exported** to open the file or **Open folder** to browse its location.

## Release 0.8.1

Navigation history now opens above the canvas footer beside Undo and Redo. File groups project creation and opening, saving, and exporting. Editor logs occupy the far-right toolbar position. Right-click any page to add a child, edit its departments or export its canvas.

## Release 0.8.0

**File** contains **New project**, **New Template Project**, **Open**, **Save** and **Export**. Save and Export also have dedicated toolbar buttons. New projects start with a form for the name, description and department colors. Creating a template opens **Template Project** with World direction, Traversal and Canvas benchmark.

Use the plus beside a page to add a child. Open its context menu to change departments or export the canvas. Navigation history and Trash live in the Pages dock. Undo and Redo are at the bottom of the canvas. The canvas plus and its empty-space context menu offer the available element types.

The top-right icons open keyboard shortcuts, editor logs, settings and update checks. The log retains the most recent 250 editor errors locally across restarts. A successful manual update check with no newer release disables the button for one hour or until the next launch.

## Release 0.7.0

Choose **Export** for a project PDF with a linked index. Right-click a page and choose **Export page as PDF** for that canvas alone. Preview the sheets, fit each canvas on one landscape A4 sheet or tile it at its original scale. Active department filters can optionally be applied.

Choose **Project archive (.zip)** to share an editable project. The archive contains the current project and its adjacent assets directory. Review original file and folder paths, including references in the trash. **Convert to local** copies a reference into the project and saves the updated project. **Convert all to local** collects all listed paths. Extract the ZIP and open the included project file on another computer. Web links keep their URLs. Copied Windows shortcut files keep their original targets.

## Release 0.6.0

Double-click a note heading to edit its **Title**. Enter accepts the change and Escape cancels it. Double-click the body to edit the note text.

**New** opens a blank project in edit mode. **Template Project** asks for a destination folder and creates a sample project with the default settings and a 100-image benchmark page. An existing file with the same name is preserved. Opening an existing project starts in read mode.

The project toolbar centers department filters and places a navigation-history menu between Back and Forward. In **Settings**, change the history limit from its default of 10 visits. Keyboard shortcuts and update checks are available from the icon buttons beside Template Project and Settings.

## Release 0.5.0

Right-click objects and expand **Departments** to inspect or change their memberships. A mixed checkbox means only some selected objects belong to that department. Choose **Inherit from page** to follow page assignments.

Undo and redo are available in edit mode from the project toolbar. Use **Ctrl/Cmd + Z** to undo and **Ctrl/Cmd + Shift + Z** or **Ctrl + Y** to redo. The current project session keeps up to 100 edit steps. Moving, resizing or rotating a selection is one step. Saving preserves history; opening or creating another project starts a new history. Undoing an attachment insertion removes its canvas card and preserves the file on disk.

## Release 0.4.0

- A lock highlights read mode. **Enable editing** or **E** unlocks editing.
- Drop files, folders, images or Windows shortcuts on the desktop canvas. **Copy to project** stores a separate copy beside the project in its matching `.assets` folder. **Link original** retains the original path. Double-click an attachment or choose **Open** from its context menu. Existing embedded images continue to work.
- Paste a web URL onto the canvas for a link card. The desktop app retrieves the page title and description when available. YouTube video links offer an embedded player. **Open** uses the default browser.
- Open the gear beside the project name for the display name, description, department names and colors. Renaming the project inside an existing file preserves the filename. Page badges show all assigned department colors. Child pages inherit their parent’s departments until overridden.
- Double-click a frame in edit mode for **Frame only**. Its handles now transform the frame independently. Double-click again or press Escape to leave this mode. Multiple selections offer separate horizontal and vertical alignment icon groups.

See [feature status](FEATURE-STATUS.md) for the remaining original requirements.

## Release 0.3.1

Drag a panel header to either highlighted edge to dock it. Use the small chevron on each panel to collapse it or reopen it from its edge tab. Panel positions and visibility are remembered. The read/edit switch sits at the right end of the project toolbar. The file toolbar starts with New, Open and Save. An asterisk on Save indicates unsaved changes.

## Release 0.3.0

- Drag the empty canvas with the left button to select enclosed objects. Shift-click to adjust the selection, middle-drag to pan, and use the selection handles to resize or rotate the group. Right-click a selection for alignment and deletion.
- Hold Ctrl during a transform to snap. Settings stores separate move, resize and rotation increments in the project. Defaults are 10 canvas units and 10 degrees. Zero disables the corresponding snap.
- Notes wrap long words and can fit text to both dimensions. Enable **Auto-fit text to box** in the inspector to reduce the displayed font when needed.
- The project toolbar holds the department filters, read/edit mode, panel visibility and history. Use **Ctrl + Shift + Left / Right** to revisit a view. Settings includes theme and panel docking on either side.
- Press **Delete** on a selected object or focused page. Confirm page deletion to move it to Trash. Child pages move up one level. Restore a page from the sidebar.

## Release 0.2.0

Windows portable builds run directly as `Tenkivo.exe` on a machine with WebView2. Read or edit mode follows you between pages. Drag a sidebar page onto the middle of another row to nest it, onto either edge to reorder it, or onto the root drop target to promote it. In edit mode, use the four square handles to resize and the round handle to rotate. Frames transform their children together. Dark mode uses neutral surfaces.

Select a word or phrase in a note (on the canvas or in the inspector), then press **Ctrl/Cmd + K**. Choose a page and optionally an object on that page. Click the underlined text to follow the link. Object destinations are selected and framed on arrival.

Select an image, frame, note or shortcut and choose **Link object** in the inspector to link the whole object. Use its arrow to navigate. Edit or remove links from the inspector. Links are stored by ID and survive renaming and reopening. Existing 0.1.0 projects can be opened directly.

## Release 0.1.0

- Open the sample art direction board or create a local project with multiple pages.
- Switch from reading to editing with **E**. Add notes, images and frames using the bottom toolbar. Drag a page from the sidebar to create a shortcut and double-click it to navigate.
- Pan by dragging the background or middle-dragging. Use the wheel to zoom at the pointer and **F** to fit the selected object or the whole page.
- Move an object fully inside a frame to group it. Drag the frame by its title to move the group. Drag an object out to detach it.
- Double-click notes to edit. Use the inspector to change object names, dimensions, text size and departments. Filter objects with the department buttons above the canvas.
- Choose a light, dark or system theme. Use **Ctrl/Cmd + S** to save a portable local project.
- Select **Add benchmark page** for a generated board with 100 raster images of different sizes, three notes and a frame. The status bar shows a rolling animation frame rate.

Projects use a versioned JSON document with a `.tkv` extension. Existing `.gdstudio` files can still be opened. Launch the Windows portable once to associate `.tkv` files with Tenkivo for your account. Double-clicking a project opens it in a new Tenkivo window. For projects with copied attachments, move the project file and the matching `.assets` folder together. Linked originals must remain accessible at their original location. Save explicitly before closing.

## Development

Install Node.js 22.12+ or 24+, pnpm 11, stable Rust and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform. On Windows these include the MSVC C++ build tools, Windows SDK and WebView2.

```sh
pnpm install --frozen-lockfile
pnpm check
pnpm desktop dev
```

For live desktop iteration on Windows, open `Live preview.cmd` from the checkout. It runs Tauri with Vite hot reload. For a portable build run `pnpm desktop build --no-bundle` on Windows and copy `src-tauri/target/release/tenkivo.exe` to a writable folder as `Tenkivo.exe`. To create an installer on request, use `pnpm desktop build --bundles nsis`.

## Online projects

Tenkivo can keep a shared project in a private GitHub repository. Each participant signs in with their own GitHub account. GitHub repository permissions determine who can read and save, and Tenkivo attributes page changes to the signed-in account.

### Set up GitHub access

1. Create a dedicated **private** repository for the project and initialize it with a README. In GitHub, install the Tenkivo GitHub App with **Only select repositories** and select that repository.
2. Select **Sign in with GitHub** on the welcome screen or in the editor's top bar. Sign in using the code and verification page. Tenkivo uses that account when you enter Edit mode for any project. Local projects can also be edited using a local profile when signed out.
3. From the welcome screen, select **New online project…**, enter the private repository as `owner/name`, and use **Create online project** to start a new shared project. **Open online project…** shows projects you can open, with a field for entering the repository name directly. To publish an existing local project, open it and choose **File → Online project… → Publish a copy**.
4. In the shared project, open **Project settings → Users on GitHub** as a repository administrator. Verify a GitHub username and then choose **Invite as editor**. Participants accept the invitation in GitHub and sign in with the same GitHub App. Private repositories they can access that contain `project.tkv` appear under **Online projects** on the welcome screen. Use **Refresh** after accepting an invitation. This settings panel can also revoke a direct collaborator's access or cancel a pending invitation. Access inherited through GitHub organization teams or roles is managed in GitHub.

Tenkivo includes the public Client ID of its GitHub App. To build against a different registration, set `TENKIVO_GITHUB_CLIENT_ID` as a local build environment variable or a GitHub Actions repository variable. That registration needs **Device Flow**, **Contents** and **Administration** repository permissions set to **Read and write**, and **Any account** installation visibility so collaborators can authorize it. On Windows, Tenkivo uses Windows Data Protection to save your session for your Windows user and renews it when needed. Choose **Sign out** from the GitHub account button to remove the saved session.

Use **Save** to commit changes to GitHub. If another participant has already saved, GitHub can reject the outdated revision. Use **Save a local copy…** to preserve your edits and **Reload from GitHub** to load the latest revision. This version has manual reload and save, with no live co-editing or merge. Online projects support embedded images and web links within a 1 MiB project document, but do not support local file or folder attachments.

## Portable releases

On Windows, double-click **Publish Tenkivo.cmd** from a clean `main` checkout. The publisher uses the existing Git credentials for `origin`. It shows the current version, suggests the next patch version and previews the release notes it generated from commits since the version last changed. After you type `PUBLISH`, it commits the version files and pushes `main` and the annotated version tag together. GitHub Actions builds and publishes the Windows portable files. Follow the run under **Actions → Publish Tenkivo for Windows**. If the push fails, fix the Git error and run the publisher again to retry the local tag. If the build or upload fails, rerun the failed job in Actions. Wait for the run to finish before starting another release.

Write commit subjects in English. A commit can include one or more `Release-Note:` trailers in English to supply user facing wording. The publisher uses these entries when present and the commit subject otherwise.

After a successful push, press Enter to close the publisher and open the GitHub Actions run in your default browser. If GitHub has not returned the run yet, the publisher opens the workflow filtered by the release tag.

To test an update, keep a single copy of the previous portable executable open in a writable folder. When the new release is live, click **Check for updates**, then **Update and restart**. A manual check that finds no newer version disables the button for one hour. When the hour ends, the app checks again automatically. The update icon shows a badge for a detected release, even after choosing **Later**.

Publish a stable public GitHub Release with a semantic version tag (e.g. `v0.2.0`), release notes and a single Windows x64 executable named `Tenkivo.exe`. The portable app checks the latest public release at startup, hourly while open, and on demand. It shows both versions and the release notes before downloading. The downloaded binary must match the SHA-256 digest from the GitHub release API. Updating requires write access to the executable's directory. Offline users can continue working.

The English product website lives in `docs/`. It can be served directly or published with GitHub Pages using the main branch and the docs folder. Its download button resolves the portable asset from the latest public release.

**Editor logs** records the running version, update checks and their outcomes, and editor errors. Tenkivo checks GitHub Releases without signing in, so private releases are inaccessible even when the GitHub Actions publishing job succeeds.
