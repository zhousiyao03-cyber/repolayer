package main

import "fmt"

import (
	"errors"
	"strings"
)

func Login(user, pwd string) bool {
	return helper(pwd) == user
}

func helper(s string) string { return s }

type Auth struct {
	id int
}

func (a *Auth) Validate() bool { return true }

func (a *Auth) internal() bool { return false }

type SmallType struct{}

type ID = int64

type smallAlias = int // unexported alias

type unexported struct{}

var _ = fmt.Sprintf
var _ = errors.New
var _ = strings.ToLower
