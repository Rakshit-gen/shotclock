import { useState } from "react";

interface Props {
  initial: string;
  busy: boolean;
  error: string | null;
  onOpen: (path: string) => void;
  onCancel?: () => void;
}

export default function OpenFolder({ initial, busy, error, onOpen, onCancel }: Props) {
  const [path, setPath] = useState(initial);
  return (
    <form
      className="open-folder"
      onSubmit={(e) => {
        e.preventDefault();
        onOpen(path);
      }}
    >
      <label htmlFor="folder-path">Folder with the photos</label>
      <div className="open-row">
        <input
          id="folder-path"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="~/Pictures/Japan 2024"
          spellCheck={false}
          autoComplete="off"
          aria-describedby={error ? "open-error" : undefined}
          aria-invalid={!!error}
        />
        <button type="submit" className="primary" disabled={busy || !path.trim()}>
          {busy ? "Reading photos" : "Open"}
        </button>
        {onCancel && (
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        )}
      </div>
      {error && (
        <p id="open-error" className="help error" role="alert">
          {error}
        </p>
      )}
    </form>
  );
}
