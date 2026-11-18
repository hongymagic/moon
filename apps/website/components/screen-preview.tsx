"use client";

import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
  DialogTrigger,
} from "@ui-sdk/react/dialog";
import { Expand, X } from "lucide-react";

type ScreenPreviewProps = {
  name: string;
  file: string;
  alt: string;
  width: number;
  height: number;
};

export default function ScreenPreview({
  name,
  file,
  alt,
  width,
  height,
}: ScreenPreviewProps) {
  return (
    <Dialog>
      <DialogTrigger
        className="screen-preview"
        aria-label={`Enlarge ${name} screenshot`}
      >
        <img
          src={file}
          width={width}
          height={height}
          alt={alt}
          loading="lazy"
        />
        <span className="expand-mark" aria-hidden="true">
          <Expand size={16} />
        </span>
      </DialogTrigger>
      <DialogContent className="screen-dialog">
        <div className="dialog-toolbar">
          <DialogTitle>{name}</DialogTitle>
          <DialogClose className="dialog-close" aria-label="Close screenshot">
            <X size={20} />
          </DialogClose>
        </div>
        <img
          src={file}
          width={width}
          height={height}
          alt={alt}
          className="full-screen"
        />
        <a href={file} target="_blank" rel="noreferrer">
          Open full-size image <span aria-hidden="true">↗</span>
        </a>
      </DialogContent>
    </Dialog>
  );
}
