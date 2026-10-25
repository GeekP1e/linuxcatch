import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  CircleCheck,
  Lightbulb,
  LoaderCircle,
  RotateCcw,
  TerminalSquare,
  Trophy,
} from "lucide-react";
import { TerminalPane } from "./TerminalPane";
import type { TerminalHandle } from "./TerminalPane";
import type {
  CheckResult,
  ConnectionStatus,
  Course,
  Group,
  Language,
  Lesson,
  LocalText,
} from "./types";
import { message, readLanguage, ui } from "./i18n";

const PROGRESS_KEY = "linuxcatch.progress.v1";
const CURRENT_LESSON_KEY = "linuxcatch.currentLesson.v1";
const POSITION_KEY = "linuxcatch.position.v1";

type CoursePosition = { lessonId: string; scrollY: number; mapOpen: boolean };

function readPosition(): CoursePosition | null {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(POSITION_KEY) ?? "null");
    if (
      value !== null &&
      typeof value === "object" &&
      "lessonId" in value &&
      typeof value.lessonId === "string" &&
      "scrollY" in value &&
      typeof value.scrollY === "number" &&
      Number.isFinite(value.scrollY) &&
      value.scrollY >= 0 &&
      "mapOpen" in value &&
      typeof value.mapOpen === "boolean"
    ) {
      return {
        lessonId: value.lessonId,
        scrollY: value.scrollY,
        mapOpen: value.mapOpen,
      };
    }
  } catch {
    /* Ignore unavailable storage or an invalid saved position. */
  }
  return null;
}

function saveCurrentLesson(id: string) {
  try {
    localStorage.setItem(CURRENT_LESSON_KEY, id);
  } catch {
    /* Navigation still works without browser storage. */
  }
}

function readCurrentLesson(): string {
  try {
    return localStorage.getItem(CURRENT_LESSON_KEY) || "pwd";
  } catch {
    return "pwd";
  }
}

function readProgress(serialized?: string | null): string[] {
  try {
    const value: unknown = JSON.parse(
      (serialized === undefined
        ? localStorage.getItem(PROGRESS_KEY)
        : serialized) ?? "[]",
    );
    return Array.isArray(value)
      ? [...new Set(value.filter((id): id is string => typeof id === "string"))]
      : [];
  } catch {
    return [];
  }
}

export default function App() {
  const [language, setLanguage] = useState<Language>(readLanguage);
  const t = ui[language];
  const [groups, setGroups] = useState<Group[]>([]);
  const [lessons, setLessons] = useState<Lesson[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);
  const [fetchRevision, setFetchRevision] = useState(0);
  const [selectedId, setSelectedId] = useState(readCurrentLesson);
  const [revision, setRevision] = useState(0);
  const [completed, setCompleted] = useState<string[]>(() => readProgress());
  const [status, setStatus] = useState<ConnectionStatus>("connecting");
  const [feedback, setFeedback] = useState<CheckResult | null>(null);
  const [error, setError] = useState<LocalText | null>(null);
  const [hintOpen, setHintOpen] = useState(false);
  const [savedPosition] = useState(readPosition);
  const positionRestored = useRef(false);
  const [mapOpen, setMapOpen] = useState(
    () => savedPosition?.lessonId === selectedId && savedPosition.mapOpen,
  );
  const [checking, setChecking] = useState(false);
  const terminal = useRef<TerminalHandle>(null);
  const checkTimer = useRef<number | undefined>(undefined);
  const lesson = lessons.find((item) => item.id === selectedId);
  const group = groups.find((item) => item.id === lesson?.group);
  const mode = lesson?.mode ?? "learn";
  const visible = lessons.filter(
    (item) => item.mode === mode && item.group === lesson?.group,
  );
  const count = lessons.filter((item) => completed.includes(item.id)).length;
  const percent = lessons.length
    ? Math.round((count / lessons.length) * 100)
    : 0;
  const lessonIndex = visible.findIndex((item) => item.id === selectedId);
  const groupIndex = groups.findIndex((item) => item.id === group?.id);
  const followingGroup = groups[groupIndex + 1];
  const nextLesson =
    visible[lessonIndex + 1] ??
    (mode === "learn"
      ? lessons.find(
          (item) => item.mode === "practice" && item.group === group?.id,
        )
      : undefined) ??
    lessons.find(
      (item) => item.mode === "learn" && item.group === followingGroup?.id,
    );
  const nextLabel =
    nextLesson?.group !== lesson?.group
      ? t.nextGroup
      : nextLesson?.mode === "practice"
        ? t.nextPractice
        : t.nextLesson;

  useEffect(() => {
    document.documentElement.lang = language;
    document.title = ui[language].pageTitle;
    try {
      localStorage.setItem("linuxcatch.language", language);
    } catch {
      /* Language selection still works. */
    }
  }, [language]);

  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    setLoadError(false);
    fetch("/api/course", { signal: controller.signal })
      .then((response) => {
        if (!response.ok) throw new Error("Сервер недоступен");
        return response.json() as Promise<Course>;
      })
      .then((course) => {
        setGroups(course.groups);
        setLessons(course.lessons);
        setSelectedId((id) =>
          course.lessons.some((item) => item.id === id)
            ? id
            : (course.lessons[0]?.id ?? "pwd"),
        );
      })
      .catch((reason: Error) => {
        if (reason.name !== "AbortError") setLoadError(true);
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [fetchRevision]);

  useEffect(() => {
    try {
      const saved = readProgress();
      const merged = [...new Set([...saved, ...completed])];
      if (merged.length !== completed.length) setCompleted(merged);
      if (merged.length !== saved.length)
        localStorage.setItem(PROGRESS_KEY, JSON.stringify(merged));
    } catch {
      /* Training still works without browser storage. */
    }
  }, [completed]);

  useEffect(() => {
    const syncProgress = (event: StorageEvent) => {
      if (event.storageArea !== localStorage || event.key !== PROGRESS_KEY)
        return;
      const saved = readProgress(event.newValue);
      setCompleted((previous) => {
        const merged = [...new Set([...previous, ...saved])];
        return merged.length === previous.length &&
          merged.length === saved.length
          ? previous
          : merged;
      });
    };
    window.addEventListener("storage", syncProgress);
    return () => window.removeEventListener("storage", syncProgress);
  }, []);

  useEffect(() => {
    if (!lessons.some((item) => item.id === selectedId)) return;
    saveCurrentLesson(selectedId);
  }, [selectedId, lessons]);

  useEffect(() => {
    if (loading || !lesson) return;
    const frame = window.requestAnimationFrame(() => {
      if (!positionRestored.current) {
        if (savedPosition?.lessonId === selectedId)
          window.scrollTo(0, savedPosition.scrollY);
        positionRestored.current = true;
        savePosition();
      }
    });
    const savePosition = () => {
      if (!positionRestored.current) return;
      try {
        localStorage.setItem(
          POSITION_KEY,
          JSON.stringify({ lessonId: selectedId, scrollY: window.scrollY, mapOpen }),
        );
      } catch {
        /* Training still works without browser storage. */
      }
    };
    const saveWhenHidden = () => {
      if (document.visibilityState === "hidden") savePosition();
    };
    window.addEventListener("scroll", savePosition, { passive: true });
    window.addEventListener("pagehide", savePosition);
    document.addEventListener("visibilitychange", saveWhenHidden);
    savePosition();
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("scroll", savePosition);
      window.removeEventListener("pagehide", savePosition);
      document.removeEventListener("visibilitychange", saveWhenHidden);
    };
  }, [loading, lesson, selectedId, mapOpen, savedPosition]);

  useEffect(() => () => window.clearTimeout(checkTimer.current), []);

  const onStatus = useCallback((value: ConnectionStatus) => {
    setStatus(value);
    if (value !== "ready") {
      setChecking(false);
      window.clearTimeout(checkTimer.current);
    }
  }, []);
  const onResult = useCallback(
    (result: CheckResult) => {
      setChecking(false);
      window.clearTimeout(checkTimer.current);
      setError(null);
      setFeedback(result);
      if (result.passed)
        setCompleted((previous) =>
          previous.includes(selectedId) ? previous : [...previous, selectedId],
        );
    },
    [selectedId],
  );
  const onError = useCallback((message: LocalText) => {
    window.clearTimeout(checkTimer.current);
    setError(message);
    setChecking(false);
  }, []);

  function select(id: string) {
    window.clearTimeout(checkTimer.current);
    saveCurrentLesson(id);
    setSelectedId(id);
    setFeedback(null);
    setError(null);
    setHintOpen(false);
    setChecking(false);
    setStatus("connecting");
    setRevision((value) => value + 1);
  }

  function selectMode(nextMode: Lesson["mode"]) {
    const next =
      lessons.find(
        (item) => item.mode === nextMode && item.group === group?.id,
      ) ?? lessons.find((item) => item.mode === nextMode);
    if (next) select(next.id);
  }

  function check() {
    if (!terminal.current?.check()) return;
    setChecking(true);
    setFeedback(null);
    setError(null);
    checkTimer.current = window.setTimeout(() => {
      setChecking(false);
      setError(message("checkTimeout"));
    }, 8_000);
  }

  return (
    <div className="app-shell">
      <div className="brand-lockup">
        <a className="brand-mark" href="/" aria-label={t.home}>
          <span>L</span>
          <span>C</span>
        </a>
        <a className="brand-logo" href="/" aria-label="LinuxCatch">
          <span>LINUX</span>CATCH
        </a>
        <span aria-hidden="true" />
      </div>

      <header className="hud-shell">
        <div className="hud-terminal-bar">
          <span className="hud-terminal-dots" aria-hidden="true">
            <i />
            <i />
            <i />
          </span>
          <span className="hud-prompt">
            <b>student@linuxcatch</b>:~${" "}
            {mode === "learn" ? "./train" : "./practice"}
          </span>
          <span className="hud-level">
            {(mode === "learn" ? t.learn : t.practice).toUpperCase()}
          </span>
        </div>
        <div
          className="hud-progress"
          role="progressbar"
          aria-label={t.progress}
          aria-valuenow={percent}
          aria-valuemin={0}
          aria-valuemax={100}
        >
          <div className="hud-progress-fill" style={{ width: `${percent}%` }} />
        </div>
        <div className="hud-command-row">
          <div className="hud-metrics">
            <span className="hud-metric-label">$ progress</span>
            <span className="hud-stat" aria-label={t.completed}>
              {count} / {lessons.length}
            </span>
            <span className="hud-stat">{group?.title[language]}</span>
          </div>
          <nav className="hud-nav" aria-label={t.modes}>
            <button
              className={`hud-action ${mode === "learn" ? "active" : ""}`}
              onClick={() => selectMode("learn")}
              disabled={!lessons.length}
              aria-label={t.learn}
              aria-pressed={mode === "learn"}
            >
              ./train
            </button>
            <button
              className={`hud-action ${mode === "practice" ? "active" : ""}`}
              onClick={() => selectMode("practice")}
              disabled={!lessons.length}
              aria-label={t.practice}
              aria-pressed={mode === "practice"}
            >
              ./practice
            </button>
            <button
              className="hud-action"
              disabled
              title={t.arenaSoon}
              aria-label={t.arena}
            >
              ./arena <span className="soon">{t.soon}</span>
            </button>
            <select
              className="hud-action language-select"
              aria-label={t.language}
              value={language}
              onChange={(event) => setLanguage(event.target.value as Language)}
            >
              <option value="ru">RU</option>
              <option value="en">EN</option>
            </select>
          </nav>
        </div>
      </header>

      <main>
        {loading ? (
          <div className="empty-state">
            <LoaderCircle className="spin" />
            <h1>{t.loading}</h1>
          </div>
        ) : loadError || !lesson ? (
          <div className="empty-state">
            <TerminalSquare size={36} />
            <h1>{t.unavailable}</h1>
            <p>{loadError ? t.loadError : t.notFound}</p>
            <button
              className="hud-action"
              onClick={() => setFetchRevision((value) => value + 1)}
            >
              {t.retry}
            </button>
          </div>
        ) : (
          <>
            <section className="goal-card" aria-labelledby="lesson-title">
              <div className="goal-heading">
                {mode === "learn" ? t.learnCommand : t.applySkills}
              </div>
              <h1 id="lesson-title">{lesson.title[language]}</h1>
              <p className="goal">{lesson.goal[language]}</p>
              {mode === "learn" && (
                <div className="lesson-command">
                  <code>{lesson.command}</code>
                </div>
              )}
              <p className="explanation">{lesson.explanation[language]}</p>
              {lesson.lab_model && <p className="hint-text">{t.modelNotice}</p>}
              {mode === "learn" && (
                <div className="lesson-reference">
                  <span>
                    <span className="reference-label">{t.syntax}</span>{" "}
                    <code>{lesson.syntax[language]}</code>
                  </span>
                  <span>
                    <span className="reference-label">{t.example}</span>{" "}
                    <code>{lesson.example}</code>
                  </span>
                </div>
              )}
              <div className="goal-tools">
                <span className="workspace-location">
                  {t.start} <code>{lesson.start_dir}</code>
                </span>
                {mode === "learn" && (
                  <button
                    className="hud-action"
                    aria-expanded={hintOpen}
                    aria-controls="lesson-hint"
                    onClick={() => setHintOpen((value) => !value)}
                  >
                    <Lightbulb size={14} />
                    {hintOpen ? t.hideHint : t.hint}
                  </button>
                )}
              </div>
              {mode === "learn" && hintOpen && (
                <p id="lesson-hint" className="hint-text">
                  {lesson.hint[language]}
                </p>
              )}
            </section>

            <section className="terminal-panel" aria-label={t.task}>
              <div className="terminal-topbar">
                <span>{t.terminal}</span>
                <span
                  className={`connection ${status === "ready" ? "connected" : ""}`}
                >
                  <i />
                  {status === "ready"
                    ? t.connected
                    : status === "connecting"
                      ? t.connecting
                      : t.disconnected}
                </span>
              </div>
              <div className="terminal-screen">
                <TerminalPane
                  key={`${selectedId}:${revision}`}
                  ref={terminal}
                  lessonId={selectedId}
                  language={language}
                  onStatus={onStatus}
                  onResult={onResult}
                  onError={onError}
                />
                <div className="terminal-statusline">
                  <strong>BASH</strong>
                  <span className="status-separator">|</span>
                  <span className="terminal-shortcuts">
                    <span>
                      <kbd>Tab</kbd> {t.completion}
                    </span>
                    <span>
                      <kbd>↑</kbd> {t.history}
                    </span>
                    <span>
                      <kbd>Ctrl C</kbd> {t.interrupt}
                    </span>
                  </span>
                </div>
              </div>
            </section>

            <div
              className={`feedback ${!error && feedback?.passed ? "success" : error || feedback ? "notice" : ""}`}
              role="status"
              aria-live="polite"
            >
              {error ? (
                <>
                  <Lightbulb size={16} />
                  <span>{error[language]}</span>
                </>
              ) : feedback ? (
                <>
                  {feedback.passed ? (
                    <CircleCheck size={16} />
                  ) : (
                    <Lightbulb size={16} />
                  )}
                  <span>{feedback.message[language]}</span>
                </>
              ) : (
                <span>
                  {status === "closed" || status === "error"
                    ? t.sessionClosed
                    : t.readyToCheck}
                </span>
              )}
              {feedback?.passed && nextLesson && (
                <button
                  className="hud-action"
                  onClick={() => select(nextLesson.id)}
                >
                  {nextLabel}
                  <ArrowRight size={14} />
                </button>
              )}
              {feedback?.passed && !nextLesson && (
                <span className="practice-complete">
                  <Trophy size={16} />
                  {t.practiceComplete}
                </span>
              )}
            </div>

            <div className="curriculum-console">
              <span className="curriculum-console-prompt">
                $ {mode === "learn" ? "lesson" : "practice"}
              </span>
              <span className="curriculum-console-progress">
                {mode === "learn" ? t.lesson : t.exercise} {lessonIndex + 1} /{" "}
                {visible.length} · ✓{" "}
                {visible.filter((item) => completed.includes(item.id)).length} /{" "}
                {visible.length}
              </span>
              <div className="curriculum-console-actions">
                <button
                  className="hud-action curriculum-arrow"
                  aria-label={t.previousLesson}
                  disabled={lessonIndex === 0}
                  onClick={() => select(visible[lessonIndex - 1].id)}
                >
                  <ArrowLeft size={14} />
                </button>
                <button
                  className="hud-action curriculum-arrow"
                  aria-label={nextLabel}
                  disabled={!nextLesson}
                  onClick={() => nextLesson && select(nextLesson.id)}
                >
                  <ArrowRight size={14} />
                </button>
                <button
                  className="hud-action"
                  aria-expanded={mapOpen}
                  aria-controls="course-map"
                  onClick={() => setMapOpen((value) => !value)}
                >
                  {t.courseMap}
                </button>
              </div>
            </div>

            <div id="course-map" className="curriculum-map" hidden={!mapOpen}>
              {groups.map((topic, index) => {
                const items = lessons.filter((item) => item.group === topic.id);
                return (
                  <details
                    className="curriculum-section"
                    key={topic.id}
                    open={topic.id === group?.id}
                  >
                    <summary>
                      <span>
                        {String(index + 1).padStart(2, "0")} ·{" "}
                        {topic.title[language]}
                      </span>
                      <span className="group-progress">
                        ✓{" "}
                        {
                          items.filter((item) => completed.includes(item.id))
                            .length
                        }{" "}
                        / {items.length}
                      </span>
                    </summary>
                    {(["learn", "practice"] as const).map((sectionMode) => (
                      <div className="curriculum-mode" key={sectionMode}>
                        <h2>
                          {sectionMode === "learn"
                            ? t.groupLearn
                            : t.groupPractice}
                        </h2>
                        <div className="course-task-grid">
                          {items
                            .filter((item) => item.mode === sectionMode)
                            .map((item, itemIndex) => (
                              <button
                                key={item.id}
                                className={`curriculum-node ${item.id === selectedId ? "current" : ""} ${completed.includes(item.id) ? "complete" : ""}`}
                                aria-current={
                                  item.id === selectedId ? "step" : undefined
                                }
                                onClick={() => select(item.id)}
                              >
                                <span className="curriculum-node-number">
                                  {completed.includes(item.id) ? (
                                    <Check size={14} />
                                  ) : (
                                    String(itemIndex + 1).padStart(2, "0")
                                  )}
                                </span>
                                <span>
                                  <strong>{item.title[language]}</strong>
                                  {sectionMode === "learn" && (
                                    <code>{item.command}</code>
                                  )}
                                </span>
                              </button>
                            ))}
                        </div>
                      </div>
                    ))}
                  </details>
                );
              })}
            </div>

            <div className="session-actions">
              <button className="hud-action" onClick={() => select(selectedId)}>
                <RotateCcw size={14} />
                {t.reset}
              </button>
              <button
                className="hud-action primary-button"
                onClick={check}
                disabled={status !== "ready" || checking}
              >
                {checking ? (
                  <LoaderCircle className="spin" size={14} />
                ) : (
                  <Check size={14} />
                )}
                {checking ? t.checking : t.check}
              </button>
            </div>
          </>
        )}
      </main>
      <footer className="app-footer">LinuxCatch · Learn. Try. Catch.</footer>
    </div>
  );
}
