import { hash } from './utils';

export function login(user: string, pass: string): boolean {
  return hash(pass) === user;
}

export function logout(): void {}

function internalHelper() {}
