#!/usr/bin/env python3
# pkgbuild_manager.py — Nautilus Python extension
# Reads menu layout from ~/.config/pkgbuild-manager/menu.json

import os
import json
import gettext
import shutil
import shlex
import subprocess
import threading
import gi
import re
import time
from urllib.parse import unquote, urlparse
from urllib.request import Request, urlopen
from pathlib import Path

gi.require_version("Nautilus", "4.1")
gi.require_version("Gdk", "4.0")
gi.require_version("Gtk", "4.0")
from gi.repository import Nautilus, GObject, Gdk, Gtk, GLib

_user_locale = os.path.expanduser("~/.local/share/locale")
_locale_dir = _user_locale if os.path.isdir(_user_locale) else "/usr/share/locale"
_language = (os.environ.get("LANGUAGE") or os.environ.get("LANG") or "en").split(":", 1)[0]
_translation = gettext.translation("pkgbuild_manager", _locale_dir, languages=[_language], fallback=True)
_ = _translation.gettext

CONFIG_FILE = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "pkgbuild-manager" / "menu.json"

# Mantém paridade com all_actions() em src/settings_gui/config.rs
DEFAULT_ACTIONS = [
    ("00_Full Workflow",     "Full Workflow"),
    ("01_Build",             "Build"),
    ("02b_Build and Clean",  "Build and Clean"),
    ("08_Build Force",       "Force Build"),
    ("09_Build NoCheck",     "Build without Checks"),
    ("10_Build NoGPG",       "Build without GPG"),
    ("11_Fetch Sources",     "Fetch Sources"),
    ("02_Install",           "Install"),
    ("12_Install Force",     "Force Install"),
    ("13_Install RmDeps",    "Install and Remove Build Dependencies"),
    ("14_Install NoCheck",   "Install without Checks"),
    ("15_Install NoGPG",     "Install without GPG"),
    ("03_Update Checksums",  "Update Checksums"),
    ("04_Update .SRCINFO",   "Update .SRCINFO"),
    ("16_Gen Checksums",     "Generate Checksums"),
    ("05_Namcap",            "Namcap"),
    ("05b_ShellCheck",       "ShellCheck"),
    ("06_Push AUR",          "Push to AUR"),
    ("17_Push AUR Tag",      "Push AUR Tag"),
    ("07_Clean srcdir",      "Clean srcdir"),
    ("07b_Clean Everything", "Clean Everything"),
]
DEFAULT_LABELS = dict(DEFAULT_ACTIONS)

ROOT_GROUP = "PKGBUILD"
TAG_ACTION = "17_Push AUR Tag"
ARCHIVE_SUFFIXES = (
    ".zip", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2",
    ".tar.xz", ".txz", ".tar.zst", ".tzst", ".7z", ".rar",
)


def _project_root(path):
    current = Path(path).resolve().parent
    for directory in (current, *current.parents):
        if (directory / ".git").exists():
            return directory
    return None


def _is_aur_repository(root):
    try:
        result = subprocess.run(
            ["git", "-C", str(root), "remote", "get-url", "origin"],
            capture_output=True, text=True, timeout=2, check=False,
        )
        return "aur.archlinux.org" in result.stdout.lower()
    except (OSError, subprocess.TimeoutExpired):
        return False


def _is_archive(path):
    return str(path).lower().endswith(ARCHIVE_SUFFIXES)


def _notify(message, error=False):
    args = ["notify-send", "-a", "PKGBUILD Manager"]
    if error:
        args.extend(["-u", "critical"])
    subprocess.Popen(args + [message], close_fds=True)


def _show_error_window(message):
    dialog = Gtk.AlertDialog()
    dialog.set_message(_("Repository download error"))
    dialog.set_detail(str(message))
    dialog.set_buttons([_("Close")])
    dialog.show(None)


def _format_duration(seconds):
    seconds = max(0, int(seconds))
    minutes, seconds = divmod(seconds, 60)
    hours, minutes = divmod(minutes, 60)
    if hours:
        return f"{hours:d}:{minutes:02d}:{seconds:02d}"
    return f"{minutes:02d}:{seconds:02d}"


class _CloneProgressWindow:
    """Small, thread-safe GTK window used while validating and cloning."""

    def __init__(self, repository_name, file_download=False):
        self._started_at = time.monotonic()
        self._clone_started_at = None
        self._last_percent = 0
        self._file_download = file_download

        window_title = _("Downloading file") if file_download else _("Cloning repository")
        self.window = Gtk.Window(title=window_title)
        self.window.set_default_size(440, -1)
        self.window.set_resizable(False)

        content = Gtk.Box(
            orientation=Gtk.Orientation.VERTICAL,
            spacing=12,
            margin_top=24,
            margin_bottom=18,
            margin_start=24,
            margin_end=24,
        )
        self.title = Gtk.Label(label=repository_name)
        self.title.add_css_class("title-2")
        self.title.set_ellipsize(3)  # Pango.EllipsizeMode.END
        initial_status = _("Preparing download…") if file_download else _("Validating repository…")
        self.status = Gtk.Label(label=initial_status)
        self.status.set_xalign(0)
        self.progress = Gtk.ProgressBar(show_text=True)
        self.progress.set_text(_("Preparing…"))
        self.progress.pulse()
        self.time_label = Gtk.Label(label=_("Elapsed time: 00:00"))
        self.time_label.set_xalign(0)
        self.time_label.add_css_class("dim-label")
        self.close_button = Gtk.Button(label=_("Close"))
        self.close_button.set_halign(Gtk.Align.END)
        self.close_button.set_sensitive(False)
        self.close_button.connect("clicked", lambda _button: self.window.close())

        content.append(self.title)
        content.append(self.status)
        content.append(self.progress)
        content.append(self.time_label)
        content.append(self.close_button)
        self.window.set_child(content)
        self.window.present()

        self._timer_id = GLib.timeout_add_seconds(1, self._tick)

    def _tick(self):
        if self.close_button.get_sensitive():
            return False
        if self._clone_started_at is None:
            self.progress.pulse()
        elapsed_total = time.monotonic() - self._started_at
        if self._clone_started_at is not None and self._last_percent > 0:
            clone_elapsed = time.monotonic() - self._clone_started_at
            remaining = clone_elapsed * (100 - self._last_percent) / self._last_percent
            self.time_label.set_label(
                _("Elapsed: {elapsed} • Remaining: about {remaining}").format(
                    elapsed=_format_duration(elapsed_total),
                    remaining=_format_duration(remaining),
                )
            )
        else:
            self.time_label.set_label(
                _("Elapsed time: {elapsed}").format(elapsed=_format_duration(elapsed_total))
            )
        return True

    def cloning_started(self):
        self._clone_started_at = time.monotonic()
        self.status.set_label(_("Downloading repository data…"))
        self.progress.set_fraction(0.0)
        self.progress.set_text("0%")

    def file_download_started(self):
        self._clone_started_at = time.monotonic()
        self.status.set_label(_("Downloading file…"))
        self.progress.set_fraction(0.0)
        self.progress.set_text("0%")

    def file_download_progress(self, downloaded, total):
        if total > 0:
            percent = min(100, round(downloaded * 100 / total))
            self._last_percent = percent
            self.progress.set_fraction(percent / 100.0)
            self.progress.set_text(f"{percent}%")
        else:
            self.progress.pulse()
            self.progress.set_text(_("Downloading…"))

    def file_download_finished(self):
        self._last_percent = 100
        self.progress.set_fraction(1.0)
        self.progress.set_text("100%")
        self.status.set_label(_("File downloaded successfully!"))
        self.progress.add_css_class("success")
        self.close_button.set_sensitive(True)

    def update(self, percent, phase):
        # Git can report a new phase starting at a lower percentage. Keep the
        # overall bar monotonic by assigning the phases portions of the clone.
        phase_start, phase_size = {
            "Counting objects": (0, 5),
            "Compressing objects": (5, 15),
            "Receiving objects": (20, 65),
            "Resolving deltas": (85, 15),
            "Updating files": (85, 15),
        }.get(phase, (0, 100))
        overall = min(100, phase_start + round(percent * phase_size / 100))
        self._last_percent = max(self._last_percent, overall)
        self.progress.set_fraction(self._last_percent / 100.0)
        self.progress.set_text(f"{self._last_percent}%")
        self.status.set_label(_(phase) + "…")

    def finish(self, success, detail=None):
        if success:
            self._last_percent = 100
            self.progress.set_fraction(1.0)
            self.progress.set_text("100%")
            self.status.set_label(_("Repository cloned successfully!"))
            self.progress.add_css_class("success")
        else:
            failure = _("Failed to download file") if self._file_download else _("Failed to clone repository")
            self.status.set_label(failure)
            self.progress.add_css_class("error")
            if detail:
                self.time_label.set_label(str(detail))
        self.close_button.set_sensitive(True)


def _repository_from_url(text):
    """Return (clone_url, optional_branch) for a copied URL or git clone command."""
    value = (text or "").strip().splitlines()[0] if (text or "").strip() else ""
    # Documentation commonly presents clone links as a complete command. Parse
    # the simple `git clone URL` form, but never execute clipboard contents.
    try:
        command = shlex.split(value.removeprefix("$ ").strip())
    except ValueError:
        return None
    requested_branch = None
    requested_depth = None
    if command[:2] == ["git", "clone"]:
        urls = []
        index = 2
        while index < len(command):
            token = command[index]
            if token in ("-b", "--branch"):
                index += 1
                if index >= len(command) or not command[index]:
                    return None
                requested_branch = command[index]
            elif token.startswith("--branch="):
                requested_branch = token.split("=", 1)[1]
                if not requested_branch:
                    return None
            elif token == "--depth":
                index += 1
                if index >= len(command) or not command[index].isdigit():
                    return None
                requested_depth = int(command[index])
            elif token.startswith("--depth="):
                depth = token.split("=", 1)[1]
                if not depth.isdigit():
                    return None
                requested_depth = int(depth)
            elif token == "--single-branch":
                pass
            elif token.startswith("-"):
                return None
            else:
                urls.append(token)
            index += 1
        if len(urls) != 1 or requested_depth == 0:
            return None
        value = urls[0]

    if re.match(r"^[^\s@]+@[^\s:]+:.+", value):
        return value, requested_branch, requested_depth

    parsed = urlparse(value)
    if parsed.scheme not in ("http", "https", "ssh", "git") or not parsed.netloc:
        return None

    parts = [part for part in parsed.path.split("/") if part]
    host = parsed.netloc.lower()
    branch = None

    if host in ("github.com", "www.github.com") and len(parts) >= 2:
        owner, repository = parts[0], parts[1].removesuffix(".git")
        if len(parts) >= 4 and parts[2] == "tree":
            branch = "/".join(parts[3:])
        return f"{parsed.scheme}://{parsed.netloc}/{owner}/{repository}.git", requested_branch or branch, requested_depth

    # GitLab uses /owner/repository/-/tree/branch.
    if "-/tree" in parsed.path:
        prefix, branch = parsed.path.split("/-/tree/", 1)
        return f"{parsed.scheme}://{parsed.netloc}{prefix}.git", requested_branch or branch.strip("/"), requested_depth

    # Codeberg/Gitea uses /owner/repository/src/branch/branch-name.
    if "/src/branch/" in parsed.path:
        prefix, branch = parsed.path.split("/src/branch/", 1)
        return f"{parsed.scheme}://{parsed.netloc}{prefix}.git", requested_branch or branch.strip("/"), requested_depth

    if len(parts) >= 2:
        # Unknown Git servers are tested as copied. Some do not accept a .git
        # suffix, so avoid rewriting URLs when their web layout is unknown.
        return value.rstrip("/"), requested_branch, requested_depth
    return None


def _safe_download_name(name):
    """Return a destination filename, rejecting paths and special names."""
    name = unquote(name or "").strip()
    if not name or name in (".", "..") or name != os.path.basename(name):
        return None
    if "/" in name or "\\" in name or "\0" in name:
        return None
    return name


def _file_download_from_text(text):
    """Parse an allowlisted download command without executing a shell."""
    value = (text or "").strip().splitlines()[0] if (text or "").strip() else ""
    try:
        tokens = shlex.split(value.removeprefix("$ ").strip())
    except ValueError:
        return None
    if not tokens or any(token in ("|", ";", "&&", "||", ">", ">>") for token in tokens):
        return None

    output_name = None
    urls = []
    program = tokens[0]
    index = 1

    if program == "curl":
        allowed = {"-f", "-s", "-S", "-L", "-O", "--fail", "--silent", "--show-error", "--location", "--remote-name"}
        while index < len(tokens):
            token = tokens[index]
            if token in ("-o", "--output"):
                index += 1
                if index >= len(tokens):
                    return None
                output_name = _safe_download_name(tokens[index])
                if output_name is None:
                    return None
            elif token in allowed:
                pass
            elif token.startswith("-") and len(token) > 2 and not token.startswith("--"):
                if any(f"-{flag}" not in allowed for flag in token[1:]):
                    return None
            elif token.startswith("-") and token not in allowed:
                return None
            elif token.startswith(("http://", "https://")):
                urls.append(token)
            else:
                return None
            index += 1
    elif program == "wget":
        allowed = {"-q", "--quiet", "--show-progress"}
        while index < len(tokens):
            token = tokens[index]
            if token in ("-O", "--output-document"):
                index += 1
                if index >= len(tokens):
                    return None
                output_name = _safe_download_name(tokens[index])
                if output_name is None:
                    return None
            elif token.startswith("--output-document="):
                output_name = _safe_download_name(token.split("=", 1)[1])
                if output_name is None:
                    return None
            elif token in allowed:
                pass
            elif token.startswith("-") and token not in allowed:
                return None
            elif token.startswith(("http://", "https://")):
                urls.append(token)
            else:
                return None
            index += 1
    elif program == "aria2c":
        while index < len(tokens):
            token = tokens[index]
            if token in ("-o", "--out"):
                index += 1
                if index >= len(tokens):
                    return None
                output_name = _safe_download_name(tokens[index])
                if output_name is None:
                    return None
            elif token.startswith("--out="):
                output_name = _safe_download_name(token.split("=", 1)[1])
                if output_name is None:
                    return None
            elif token.startswith("-"):
                return None
            elif token.startswith(("http://", "https://")):
                urls.append(token)
            else:
                return None
            index += 1
    elif len(tokens) == 1 and tokens[0].startswith(("http://", "https://")):
        urls = tokens
    else:
        return None

    if len(urls) != 1:
        return None
    url = urls[0]
    parsed = urlparse(url)
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        return None
    inferred_name = _safe_download_name(os.path.basename(parsed.path.rstrip("/")))
    return (url, output_name or inferred_name) if output_name or inferred_name else None


def _clipboard_download(text):
    """Return ('git', repository) or ('file', (url, filename))."""
    value = (text or "").strip()
    command = value.removeprefix("$ ").strip()
    if command.startswith("git clone "):
        repository = _repository_from_url(value)
        if repository is not None:
            return "git", repository

    file_download = _file_download_from_text(value)
    if file_download is not None:
        # A raw .git URL is unambiguously a repository. Other raw URLs are
        # treated as files; web project URLs can still use `git clone URL`.
        if len(shlex.split(command)) == 1 and urlparse(file_download[0]).path.endswith(".git"):
            repository = _repository_from_url(value)
            return ("git", repository) if repository else None
        return "file", file_download

    # A copied command may contain execution operators or unsafe options. In
    # that case salvage exactly one plain HTTP(S) URL and discard everything
    # else. Nothing from the original command is executed or forwarded.
    try:
        tokens = shlex.split(command)
    except ValueError:
        return None
    urls = []
    for token in tokens:
        parsed = urlparse(token)
        if parsed.scheme in ("http", "https") and parsed.netloc:
            urls.append(token)
    if len(urls) != 1:
        return None

    url = urls[0]
    if tokens[:2] == ["git", "clone"] or urlparse(url).path.endswith(".git"):
        # Re-parse the URL alone, thereby dropping options such as
        # --upload-pack and all shell syntax around it.
        repository = _repository_from_url(url)
        return ("git", repository) if repository else None

    filename = _safe_download_name(os.path.basename(urlparse(url).path.rstrip("/")))
    return ("file", (url, filename)) if filename else None


def _repository_name(clone_url):
    path = clone_url.rstrip("/")
    # Handles both normal URLs and SCP-style SSH addresses (git@host:repo).
    name = re.split(r"[/:]", path)[-1].removesuffix(".git").strip()
    return name or None


def _repository_is_accessible(repository):
    """Return (accessible, diagnostic) without prompting for credentials."""
    clone_url, branch, _depth = repository
    command = ["git", "ls-remote", "--exit-code", clone_url]
    if branch:
        command.append(f"refs/heads/{branch}")

    environment = os.environ.copy()
    environment["GIT_TERMINAL_PROMPT"] = "0"
    environment["GIT_ASKPASS"] = "true"
    try:
        result = subprocess.run(
            command,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
            env=environment,
            timeout=30,
        )
        if result.returncode == 0:
            return True, None
        detail = (result.stderr or "").strip().splitlines()
        return False, detail[-1] if detail else _("Git rejected the repository URL.")
    except subprocess.TimeoutExpired:
        return False, _("Repository validation timed out after 30 seconds.")
    except OSError as error:
        return False, str(error)


def _validate_and_clone(repository, destination, progress=None):
    """Validate first, then clone only repositories Git can access."""
    accessible, diagnostic = _repository_is_accessible(repository)
    if not accessible:
        if progress is not None:
            GLib.idle_add(progress.finish, False, diagnostic)
        GLib.idle_add(
            _show_error_window,
            _("The copied URL is not an accessible Git repository.")
            + "\n\n"
            + (diagnostic or _("Unknown Git error")),
        )
        return
    _clone_repository(repository, destination, progress=progress)


def _clone_repository(repository, destination, completed=None, progress=None):
    clone_url, branch, depth = repository
    # --progress forces machine-readable progress on stderr even though the
    # extension is not attached to a terminal.
    command = ["git", "clone", "--progress"]
    if depth:
        command.extend(["--depth", str(depth)])
    if branch:
        command.extend(["--branch", branch, "--single-branch"])
    name = _repository_name(clone_url) or _("Git repository")
    cloned_path = os.path.join(destination, name)
    created_destination = False
    try:
        # Creating the top-level directory explicitly gives Nautilus a simple,
        # immediate filesystem event. Git can clone into an existing empty
        # directory and then populate it in the background.
        os.mkdir(cloned_path)
        created_destination = True
        command.extend([clone_url, cloned_path])
    except FileExistsError:
        # Preserve git-clone's normal error handling and diagnostics when the
        # inferred repository directory already exists.
        command.append(clone_url)

    if progress is not None:
        GLib.idle_add(progress.cloning_started)

    diagnostics = []
    try:
        process = subprocess.Popen(
            command,
            cwd=destination,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=1,
        )
        # Git refreshes progress with carriage returns, so iterate over both
        # CR and LF records rather than waiting for newline-only output.
        pending = ""
        while True:
            char = process.stderr.read(1)
            if not char:
                if pending:
                    diagnostics.append(pending)
                break
            if char in "\r\n":
                line, pending = pending.strip(), ""
                if not line:
                    continue
                diagnostics.append(line)
                match = re.search(r"(Counting objects|Compressing objects|Receiving objects|Resolving deltas|Updating files):\s+(\d+)%", line)
                if match and progress is not None:
                    GLib.idle_add(progress.update, int(match.group(2)), match.group(1))
            else:
                pending += char
        stdout = process.stdout.read() if process.stdout else ""
        returncode = process.wait()
    except OSError as error:
        returncode, stdout = -1, ""
        diagnostics.append(str(error))

    if returncode == 0:
        if progress is not None:
            GLib.idle_add(progress.finish, True)
        GLib.idle_add(_notify, _("✓ Repository downloaded successfully: ") + name, False)
        if completed is not None:
            # The clone runs outside GTK's main thread.  Marshal completion
            # back to GLib before touching the MenuProvider/Nautilus.
            GLib.idle_add(completed, cloned_path)
    else:
        if created_destination:
            shutil.rmtree(cloned_path, ignore_errors=True)
        detail = diagnostics or stdout.strip().splitlines()
        message = detail[-1] if detail else _("Unknown Git error")
        if progress is not None:
            GLib.idle_add(progress.finish, False, message)
        GLib.idle_add(_show_error_window, message)


def _download_file(download, destination, progress=None):
    """Download one HTTP(S) file without invoking curl, wget, or a shell."""
    url, filename = download
    target = Path(destination) / filename
    partial = Path(destination) / f".{filename}.part"
    if target.exists() or partial.exists():
        message = _("A file with this name already exists: {name}").format(name=filename)
        if progress is not None:
            GLib.idle_add(progress.finish, False, message)
        GLib.idle_add(_show_error_window, message)
        return

    try:
        request = Request(url, headers={"User-Agent": "PKGBUILD-Manager/2.4"})
        with urlopen(request, timeout=30) as response:
            final_url = urlparse(response.geturl())
            if final_url.scheme not in ("http", "https"):
                raise ValueError(_("The download redirected to an unsupported address."))
            total = int(response.headers.get("Content-Length") or 0)
            downloaded = 0
            if progress is not None:
                GLib.idle_add(progress.file_download_started)
            with partial.open("xb") as output:
                while True:
                    chunk = response.read(256 * 1024)
                    if not chunk:
                        break
                    output.write(chunk)
                    downloaded += len(chunk)
                    if progress is not None:
                        GLib.idle_add(progress.file_download_progress, downloaded, total)
            if total > 0 and downloaded != total:
                raise OSError(_("The download ended before the complete file was received."))
        # Publish atomically without overwriting a file created while the
        # download was running. Both paths are in the same destination folder.
        os.link(partial, target)
        partial.unlink()
    except Exception as error:
        try:
            partial.unlink(missing_ok=True)
        except OSError:
            pass
        if progress is not None:
            GLib.idle_add(progress.finish, False, str(error))
        GLib.idle_add(_show_error_window, str(error))
        return

    if progress is not None:
        GLib.idle_add(progress.file_download_finished)
    GLib.idle_add(_notify, _("✓ File downloaded successfully: ") + filename, False)


def _scripts_dir():
    here = os.path.dirname(os.path.abspath(__file__))
    # A system extension must use the scripts shipped by the same native
    # package. Per-user scripts may be stale leftovers from an older Flatpak.
    if here.startswith("/usr/share/"):
        installed = "/usr/share/pkgbuild-manager/scripts"
        if os.path.isdir(installed):
            return installed
    if here.startswith("/usr/local/share/"):
        installed = "/usr/local/share/pkgbuild-manager/scripts"
        if os.path.isdir(installed):
            return installed
    user_flatpak = os.path.expanduser("~/.local/share/pkgbuild-manager/scripts")
    if os.path.isdir(user_flatpak):
        return user_flatpak
    installed = "/usr/share/pkgbuild-manager/scripts"
    if os.path.isdir(installed):
        return installed
    return os.path.normpath(os.path.join(here, "..", "nautilus-scripts"))


def _load_menu():
    if CONFIG_FILE.exists():
        try:
            data = json.loads(CONFIG_FILE.read_text())
            result = []
            for group in data:
                for item in group.get("items", []):
                    if item.get("enabled", True):
                        sid = item["id"]
                        label = item["label"]
                        if label == sid:
                            label = _(DEFAULT_LABELS.get(sid, label))
                        result.append((sid, label, group["group"]))
            return result, len(data)
        except Exception:
            pass
    return [(sid, _(default_label), ROOT_GROUP) for sid, default_label in DEFAULT_ACTIONS], 1


class PkgbuildMenuProvider(GObject.GObject, Nautilus.MenuProvider):

    def __init__(self):
        super().__init__()
        self._destination = None
        self._download_item = Nautilus.MenuItem(
            name="PkgbuildManager::CloneClipboardRepository",
            label=_("Download from clipboard"),
            tip=_("Download a Git repository or file from a safe clipboard command"),
        )
        self._download_item.connect("activate", self._download_repository)

        display = Gdk.Display.get_default()
        self._clipboard = display.get_clipboard() if display else None
        self._download_item.set_property("sensitive", self._clipboard is not None)

    def _download_repository(self, _item):
        destination = self._destination
        if self._clipboard is None or not destination:
            return

        def clipboard_ready(source, result, _data=None):
            try:
                text = source.read_text_finish(result)
            except GLib.Error:
                text = None
            download = _clipboard_download(text)
            if download is None:
                _show_error_window(
                    _("Copy a supported Git, curl, wget, aria2c, or HTTP(S) download before using this option.")
                )
                return
            kind, request = download
            name = (_repository_name(request[0]) or _("Git repository")) if kind == "git" else request[1]
            progress = _CloneProgressWindow(name, file_download=(kind == "file"))
            target = _validate_and_clone if kind == "git" else _download_file
            threading.Thread(
                target=target,
                args=(request, destination, progress),
                daemon=True,
            ).start()

        self._clipboard.read_text_async(None, clipboard_ready, None)

    def _build_menu(self, pkgbuild_path, tag_only=False):
        scripts = _scripts_dir()
        items, num_groups = _load_menu()
        root = Path(pkgbuild_path) if Path(pkgbuild_path).is_dir() else Path(pkgbuild_path).parent
        if _is_aur_repository(root):
            items = [item for item in items if item[0] != TAG_ACTION]
        elif tag_only:
            items = [item for item in items if item[0] == TAG_ACTION]
        if not items:
            return []

        groups = {}
        group_order = []
        for sid, label, group in items:
            if group not in groups:
                groups[group] = []
                group_order.append(group)
            groups[group].append((sid, label))

        top = Nautilus.MenuItem(
            name="PkgbuildManager::TopMenu",
            label="PKGBUILD",
            tip="PKGBUILD Manager actions",
        )
        top_submenu = Nautilus.Menu()
        top.set_submenu(top_submenu)

        def make_cb(spath, pkgpath):
            def cb(_item):
                if not os.path.isfile(spath) or not os.access(spath, os.X_OK):
                    subprocess.Popen([
                        "notify-send", "-a", "PKGBUILD Manager",
                        "Script not found",
                        f"Missing: {spath}\nReinstall pkgbuild-manager."
                    ], close_fds=True)
                    return
                workdir = pkgpath if os.path.isdir(pkgpath) else os.path.dirname(pkgpath)
                subprocess.Popen(["bash", spath, pkgpath],
                                 cwd=workdir, close_fds=True)
            return cb

        def append_items_flat(target_menu, group_name):
            for sid, label in groups.get(group_name, []):
                sp = os.path.join(scripts, sid)
                it = Nautilus.MenuItem(
                    name=f"PkgbuildManager::{sid.replace(' ', '_')}",
                    label=label,
                    tip=f"Run {sid}"
                )
                it.connect("activate", make_cb(sp, pkgbuild_path))
                target_menu.append_item(it)

        # Grupos extras são todos exceto o grupo raiz "PKGBUILD"
        extra_groups = [g for g in group_order if g != ROOT_GROUP]

        if not extra_groups:
            # Caso simples: só o grupo raiz — itens diretos no top_submenu
            append_items_flat(top_submenu, group_order[0] if group_order else ROOT_GROUP)
        else:
            # Itens do grupo raiz vão direto no top_submenu (sem criar submenu extra)
            if ROOT_GROUP in groups:
                append_items_flat(top_submenu, ROOT_GROUP)
                # Separador visual entre itens raiz e subgrupos extras
                sep = Nautilus.MenuItem(
                    name="PkgbuildManager::Sep",
                    label="",
                    tip=""
                )
                top_submenu.append_item(sep)
            # Grupos extras viram submenus aninhados
            for gname in extra_groups:
                git = Nautilus.MenuItem(
                    name=f"PkgbuildManager::G_{gname.replace(' ', '_')}",
                    label=gname,
                    tip=gname
                )
                gsub = Nautilus.Menu()
                git.set_submenu(gsub)
                append_items_flat(gsub, gname)
                top_submenu.append_item(git)

        return [top]

    def _check_file(self, files):
        if len(files) != 1:
            return None
        f = files[0]
        if not f.get_uri().startswith("file://"):
            return None
        if f.is_directory():
            return None
        path = f.get_location().get_path()
        if f.get_name() == "PKGBUILD":
            return path, False
        if _is_archive(path):
            root = _project_root(path)
            if root is not None and not _is_aur_repository(root):
                return str(root), True
        return None

    def get_file_items(self, files):
        context = self._check_file(files)
        return self._build_menu(*context) if context else []

    def get_background_items(self, folder):
        if not folder.get_uri().startswith("file://"):
            return []
        self._destination = folder.get_location().get_path()
        if not self._destination:
            return []

        # This action is intentionally static. Nautilus caches background menu
        # providers until the view changes, so URL reading and remote validation
        # happen only after activation and never depend on a menu refresh.
        return [self._download_item]
