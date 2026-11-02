import { describe, expect, test } from "bun:test"

import { exposedPort } from "@/model/dockerfile"

const port = (...lines: string[]) => exposedPort(lines.join("\n"))

describe("the port a Dockerfile exposes", () => {
  test("is the first TCP port of the last stage, from `8080/tcp`, lists and ranges", () => {
    expect(port("FROM node", "EXPOSE 3000")).toBe("3000")
    expect(port("FROM node", "expose 8080/tcp 9090")).toBe("8080")
    expect(port("FROM node", "EXPOSE 53/udp 8443/TCP")).toBe("8443")
    expect(port("FROM node", "EXPOSE 8000-8010")).toBe("8000")
    expect(port("FROM node", "EXPOSE 70000")).toBe("")
    expect(port("FROM golang AS build", "EXPOSE 1111", "FROM alpine", "EXPOSE 2222")).toBe("2222")
    expect(port("FROM golang AS build", "EXPOSE 1111", "FROM alpine", "CMD x")).toBe("")
    expect(port("EXPOSE 3000")).toBe("")
  })

  test("joins continuation lines and leaves out comments, also inside them", () => {
    expect(port("FROM node", "# EXPOSE 3000", "EXPOSE \\", "  3100")).toBe("3100")
    expect(port("FROM node", "EXPOSE \\", "# 3000", "", "  3200")).toBe("3200")
    expect(port("FROM node", "EXPOSE 32\\", "00")).toBe("3200")
    expect(port("FROM node\r", "EXPOSE 3300\r")).toBe("3300")
  })

  test("follows the escape directive at the top only", () => {
    expect(port("# escape=`", "FROM node", "EXPOSE `", "  3400")).toBe("3400")
    expect(
      port("# syntax=docker/dockerfile:1", "#escape = `", "FROM node", "EXPOSE `", "3500"),
    ).toBe("3500")
    expect(port("FROM node", "# escape=`", "EXPOSE \\", "  3600")).toBe("3600")
  })

  test("does not read a heredoc's lines as instructions", () => {
    expect(port("FROM node", "RUN <<EOF", "EXPOSE 1111", "EOF", "EXPOSE 3700")).toBe("3700")
    expect(port("FROM node", 'COPY <<-"END" /a', "\tEXPOSE 1111", "\tEND", "EXPOSE 3800")).toBe(
      "3800",
    )
    expect(port("FROM node", "RUN cat <<<x", "EXPOSE 3900")).toBe("3900")
  })

  test("fills in ENV and ARG, with ENV over an ARG of the same name", () => {
    expect(port("FROM node", "ENV APP_PORT 5000", "EXPOSE ${APP_PORT}")).toBe("5000")
    expect(port("FROM node", 'ENV A=1 B="6000"', "EXPOSE $B")).toBe("6000")
    expect(port("FROM node", 'ENV PORTS="80 443"', "EXPOSE $PORTS")).toBe("80")
    expect(port("FROM node", "ARG PORT=4100", "EXPOSE $PORT")).toBe("4100")
    expect(port("FROM node", "ENV PORT=4200", "ARG PORT=1111", "EXPOSE $PORT")).toBe("4200")
    expect(port("FROM node", "ENV A=4300 B=$A", "EXPOSE $B")).toBe("")
    expect(port("FROM node", "ENV A=4300", "ENV B=$A", "EXPOSE $B")).toBe("4300")
  })

  test("takes a global ARG in a stage only where the stage declares it again", () => {
    expect(port("ARG PORT=4000", "FROM node", "EXPOSE $PORT")).toBe("")
    expect(port("ARG PORT=4000", "FROM node", "ARG PORT", "EXPOSE $PORT")).toBe("4000")
    expect(port("ARG BASE=node", "FROM $BASE AS app", "FROM app", "EXPOSE 4400")).toBe("4400")
  })

  test("keeps a stage's ENV, ARG and ports in the stages built from it, and no others", () => {
    expect(port("FROM node AS base", "ENV PORT=4500", "FROM base", "EXPOSE $PORT")).toBe("4500")
    expect(port("FROM node AS Base", "ARG PORT=4600", "FROM base", "EXPOSE $PORT")).toBe("4600")
    expect(port("FROM node AS base", "EXPOSE 4700", "FROM --platform=x base", "CMD x")).toBe("4700")
    expect(port("FROM node AS base", "ENV PORT=1111", "FROM alpine", "EXPOSE $PORT")).toBe("")
  })

  test("follows ${name:-word}, ${name-word}, ${name:+word} and escaped dollars", () => {
    expect(port("FROM node", "EXPOSE ${PORT:-7000}")).toBe("7000")
    expect(port("FROM node", "ENV PORT=", "EXPOSE ${PORT:-7100}")).toBe("7100")
    expect(port("FROM node", "ENV PORT=", "EXPOSE ${PORT-7200} 7300")).toBe("7300")
    expect(port("FROM node", "ENV TLS=1", "EXPOSE ${TLS:+8443} 80")).toBe("8443")
    expect(port("FROM node", "EXPOSE ${TLS:+8443} 80")).toBe("80")
    expect(port("FROM node", "ENV PORT=1111", "EXPOSE \\$PORT 7400")).toBe("7400")
    expect(port("FROM node", "ENV PORT=1111", "ENV A='$PORT'", "EXPOSE $A 7500")).toBe("7500")
  })

  test("skips a port it cannot work out", () => {
    expect(port("FROM node", "ARG PORT", "EXPOSE $PORT")).toBe("")
    expect(port("FROM node", "ARG PORT", "EXPOSE $PORT 9000")).toBe("9000")
    expect(port("FROM node", "ENV A=b\\ c", "EXPOSE $A 9100")).toBe("9100")
  })
})

describe("the words of an instruction", () => {
  test("keep quoted and escaped spaces, and the escape before other characters in double quotes", () => {
    expect(port("FROM node", 'ENV A="b c" D=e\\ f', 'EXPOSE "$A" 9200')).toBe("9200")
    expect(port("FROM node", 'ENV P="\\9300"', "EXPOSE $P 9400")).toBe("9400")
    expect(port("FROM node", 'ENV P="9\\500"', "EXPOSE $P")).toBe("")
    expect(port("FROM node", "ENV P=9\\600", "EXPOSE $P")).toBe("9600")
  })
})
